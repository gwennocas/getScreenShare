use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const REPOSITORY: &str = "https://api.github.com/repos/gwennocas/getScreenShare/releases/latest";
const MAX_UPDATE_BYTES: usize = 100 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Available {
    pub version: String,
    download_url: String,
    checksum_url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("getScreenShare/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "Não foi possível iniciar o verificador de atualizações.".into())
}

pub async fn check() -> Result<Option<Available>, String> {
    let release: Release = client()?
        .get(REPOSITORY)
        .send()
        .await
        .map_err(|_| "Não foi possível consultar as atualizações no GitHub.".to_owned())?
        .error_for_status()
        .map_err(|_| "O GitHub não retornou uma versão disponível.".to_owned())?
        .json()
        .await
        .map_err(|_| "A resposta de atualização é inválida.".to_owned())?;
    if release.draft || release.prerelease {
        return Err("A versão publicada não é uma versão estável.".into());
    }
    let latest = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let latest_version = semver::Version::parse(latest)
        .map_err(|_| "A versão publicada tem um formato inválido.".to_owned())?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| "A versão instalada tem um formato inválido.".to_owned())?;
    if latest_version <= current {
        return Ok(None);
    }
    let binary = format!(
        "getScreenShare-Rust-{}-windows-x86_64.exe",
        release.tag_name
    );
    let checksum = format!("{binary}.sha256");
    let find = |name: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
    };
    Ok(Some(Available {
        version: latest_version.to_string(),
        download_url: find(&binary).ok_or("A versão nova ainda não tem executável Windows.")?,
        checksum_url: find(&checksum).ok_or("A versão nova ainda não tem verificação SHA-256.")?,
    }))
}

pub async fn download(update: Available) -> Result<PathBuf, String> {
    let client = client()?;
    let checksum = client
        .get(&update.checksum_url)
        .send()
        .await
        .map_err(|_| "Não foi possível baixar a verificação do executável.".to_owned())?
        .error_for_status()
        .map_err(|_| "A versão não tem uma verificação válida.".to_owned())?
        .text()
        .await
        .map_err(|_| "A verificação do executável está inválida.".to_owned())?;
    let expected = checksum.trim().split_whitespace().next().unwrap_or("");
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("A verificação SHA-256 tem um formato inválido.".into());
    }
    let mut response = client
        .get(&update.download_url)
        .send()
        .await
        .map_err(|_| "Não foi possível baixar a atualização.".to_owned())?
        .error_for_status()
        .map_err(|_| "O download da atualização falhou.".to_owned())?;
    if response
        .content_length()
        .is_some_and(|size| size as usize > MAX_UPDATE_BYTES)
    {
        return Err("O executável de atualização excede o limite de tamanho.".into());
    }
    let mut bytes = Vec::with_capacity(response.content_length().unwrap_or(0) as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "O download da atualização foi interrompido.".to_owned())?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_UPDATE_BYTES {
            return Err("O executável de atualização excede o limite de tamanho.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if !bytes.starts_with(b"MZ") {
        return Err("O arquivo baixado não é um executável Windows.".into());
    }
    if !checksum_matches(&bytes, expected) {
        return Err("A verificação SHA-256 da atualização falhou.".into());
    }
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("Não foi possível localizar a pasta local do Windows.")?
        .join("getScreenShare-rust")
        .join("updates");
    fs::create_dir_all(&directory).map_err(|_| "Não foi possível preparar a atualização.")?;
    let path = directory.join("getScreenShare-rust-update.exe");
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)
        .map_err(|_| "Não foi possível gravar a atualização.")?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| "Não foi possível gravar a atualização.")?;
    if path.exists() {
        fs::remove_file(&path).map_err(|_| "Não foi possível substituir um download anterior.")?;
    }
    temporary
        .persist(&path)
        .map_err(|_| "Não foi possível concluir o download da atualização.")?;
    Ok(path)
}

fn checksum_matches(bytes: &[u8], expected: &str) -> bool {
    expected.len() == 64
        && expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        && format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(expected)
}

fn quote_powershell(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

pub fn schedule_install(update: &Path) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|_| "Não foi possível localizar o aplicativo instalado.")?;
    let staging = executable.with_extension("exe.update");
    let process_id = std::process::id();
    let script = std::env::temp_dir().join(format!("gss-update-{process_id}.ps1"));
    let body = format!(
        "$ErrorActionPreference = 'Stop'\n$processIdToWait = {process_id}\n$source = {}\n$staging = {}\n$destination = {}\nwhile (Get-Process -Id $processIdToWait -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 200 }}\nMove-Item -LiteralPath $source -Destination $staging -Force\nif (Test-Path -LiteralPath $destination) {{ [IO.File]::Replace($staging, $destination, $null) }} else {{ [IO.File]::Move($staging, $destination) }}\nStart-Process -FilePath $destination\nRemove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force\n",
        quote_powershell(update),
        quote_powershell(&staging),
        quote_powershell(&executable),
    );
    let mut content = vec![0xEF, 0xBB, 0xBF];
    content.extend_from_slice(body.as_bytes());
    fs::write(&script, content).map_err(|_| "Não foi possível preparar a instalação.")?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|_| "Não foi possível iniciar a instalação da atualização.")?;
    }
    #[cfg(not(windows))]
    let _ = Command::new("powershell.exe");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_paths_escape_quotes_as_literals() {
        assert_eq!(
            quote_powershell(Path::new("C:\\Users\\Ana O'Neil\\app.exe")),
            "'C:\\Users\\Ana O''Neil\\app.exe'"
        );
    }

    #[test]
    fn checksum_requires_exact_sha256() {
        let digest = format!("{:x}", Sha256::digest(b"package"));
        assert!(checksum_matches(b"package", &digest));
        assert!(!checksum_matches(b"tampered", &digest));
        assert!(!checksum_matches(b"package", "bad"));
    }
}
