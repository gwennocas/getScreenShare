use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const LATEST_MANIFEST: &str =
    "https://github.com/gwennocas/getScreenShare/releases/latest/download/latest.yml";
const RELEASE_DOWNLOADS: &str = "https://github.com/gwennocas/getScreenShare/releases/download";
const MAX_UPDATE_BYTES: usize = 300 * 1024 * 1024;

#[derive(Clone)]
pub struct Available {
    pub version: String,
    download_url: String,
    checksum_url: String,
}

fn manifest_version(manifest: &str) -> Option<&str> {
    manifest
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "version").then_some(value.trim())
        })
        .filter(|version| !version.is_empty())
}

pub async fn check() -> Result<Option<Available>, String> {
    let manifest = client()?
        .get(LATEST_MANIFEST)
        .send()
        .await
        .map_err(|_| "Não foi possível consultar as atualizações no GitHub.".to_string())?
        .error_for_status()
        .map_err(|_| "O GitHub não retornou uma versão disponível.".to_string())?
        .text()
        .await
        .map_err(|_| "A resposta de atualização é inválida.".to_string())?;
    let latest = manifest_version(&manifest).ok_or("A resposta de atualização é inválida.")?;
    let latest = semver::Version::parse(latest)
        .map_err(|_| "A versão publicada tem um formato inválido.".to_string())?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|_| "A versão instalada tem um formato inválido.".to_string())?;
    if latest <= current {
        return Ok(None);
    }
    let version = latest.to_string();
    let tag = format!("v{version}");
    let installer = format!("getScreenShare-Tauri-Setup-{tag}.exe");
    let release_url = format!("{RELEASE_DOWNLOADS}/{tag}");
    Ok(Some(Available {
        version,
        download_url: format!("{release_url}/{installer}"),
        checksum_url: format!("{release_url}/{installer}.sha256"),
    }))
}

pub async fn download(
    update: &Available,
    on_progress: impl Fn(u8) + Send + Sync,
) -> Result<PathBuf, String> {
    let client = client()?;
    let checksum = client
        .get(&update.checksum_url)
        .send()
        .await
        .map_err(|_| "Não foi possível baixar a verificação do instalador.".to_string())?
        .error_for_status()
        .map_err(|_| "A versão não tem uma verificação válida.".to_string())?
        .text()
        .await
        .map_err(|_| "A verificação do instalador está inválida.".to_string())?;
    let expected = checksum.trim().split_whitespace().next().unwrap_or("");
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("A verificação SHA-256 tem um formato inválido.".into());
    }
    let mut response = client
        .get(&update.download_url)
        .send()
        .await
        .map_err(|_| "Não foi possível baixar a atualização.".to_string())?
        .error_for_status()
        .map_err(|_| "O download da atualização falhou.".to_string())?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_UPDATE_BYTES as u64)
    {
        return Err("O instalador excede o limite de tamanho.".into());
    }
    let total = response.content_length();
    let mut received = 0usize;
    let mut bytes = Vec::with_capacity(total.unwrap_or(0) as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "O download da atualização foi interrompido.".to_string())?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_UPDATE_BYTES {
            return Err("O instalador excede o limite de tamanho.".into());
        }
        received += chunk.len();
        bytes.extend_from_slice(&chunk);
        if let Some(total) = total.filter(|size| *size > 0) {
            on_progress(((received as u64 * 100 / total).min(99)) as u8);
        }
    }
    if !bytes.starts_with(b"MZ") || !checksum_matches(&bytes, expected) {
        return Err("A verificação do instalador falhou.".into());
    }
    let directory = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("Não foi possível localizar a pasta local do Windows.")?
        .join("getScreenShare")
        .join("updates");
    fs::create_dir_all(&directory).map_err(|_| "Não foi possível preparar a atualização.")?;
    let path = directory.join(format!("getScreenShare-update-{}.exe", update.version));
    if path.exists() {
        fs::remove_file(&path).map_err(|_| "Não foi possível substituir um download anterior.")?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)
        .map_err(|_| "Não foi possível gravar a atualização.")?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| "Não foi possível gravar a atualização.")?;
    temporary
        .persist(&path)
        .map_err(|_| "Não foi possível concluir o download da atualização.")?;
    Ok(path)
}

#[cfg(windows)]
pub fn schedule_install(installer: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let process_id = std::process::id();
    let script = std::env::temp_dir().join(format!("gss-tauri-update-{process_id}.ps1"));
    let body = format!(
        "$ErrorActionPreference = 'Stop'\n$processIdToWait = {process_id}\n$installer = {}\nwhile (Get-Process -Id $processIdToWait -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 200 }}\nStart-Process -FilePath $installer -ArgumentList '/S' -Wait\nRemove-Item -LiteralPath $installer -Force\nRemove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force\n",
        quote_powershell(installer),
    );
    let mut content = vec![0xEF, 0xBB, 0xBF];
    content.extend_from_slice(body.as_bytes());
    fs::write(&script, content).map_err(|_| "Não foi possível preparar a instalação.")?;
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
        .arg(script)
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|_| "Não foi possível iniciar a instalação da atualização.")?;
    Ok(())
}

#[cfg(not(windows))]
pub fn schedule_install(_installer: &Path) -> Result<(), String> {
    Err("A atualização pelo aplicativo ainda está disponível apenas no Windows.".into())
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("getScreenShare/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "Não foi possível iniciar o verificador de atualizações.".into())
}

fn checksum_matches(bytes: &[u8], expected: &str) -> bool {
    expected.len() == 64
        && expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        && format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(expected)
}

fn quote_powershell(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::manifest_version;

    #[test]
    fn reads_version_from_release_manifest() {
        assert_eq!(manifest_version("version: 0.4.4\nfiles:\n"), Some("0.4.4"));
        assert_eq!(manifest_version("files:\n"), None);
    }
}
