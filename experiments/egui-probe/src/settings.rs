use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    },
};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub name: String,
    pub room: String,
    pub server: String,
    pub protected_code: Vec<u8>,
    pub share_audio: bool,
    pub fps_mode: u8,
    pub camera_device_id: String,
    pub show_stats: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: String::new(),
            room: String::new(),
            server: String::new(),
            protected_code: Vec::new(),
            share_audio: false,
            fps_mode: 0,
            camera_device_id: String::new(),
            show_stats: true,
        }
    }
}

pub fn path() -> Result<PathBuf, String> {
    std::env::var_os("APPDATA")
        .map(|dir| {
            PathBuf::from(dir)
                .join("getScreenShare-rust")
                .join("settings.json")
        })
        .ok_or("Não foi possível localizar a pasta de configurações.".into())
}

pub fn read(path: &Path) -> Result<Settings, String> {
    match fs::read(path) {
        Ok(data) => serde_json::from_slice(&data)
            .map_err(|_| "Não foi possível ler as configurações salvas.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let legacy = path
                .parent()
                .and_then(Path::parent)
                .map(|appdata| appdata.join("getScreenShare").join("native-settings.json"));
            match legacy.map(fs::read) {
                Some(Ok(data)) => serde_json::from_slice(&data).map_err(|_| {
                    "As configurações importadas do aplicativo anterior são inválidas.".into()
                }),
                Some(Err(error)) if error.kind() != std::io::ErrorKind::NotFound => {
                    Err("Não foi possível importar as configurações do aplicativo anterior.".into())
                }
                _ => Ok(Settings::default()),
            }
        }
        Err(_) => Err("Não foi possível abrir as configurações salvas.".into()),
    }
}

pub fn write(path: &Path, settings: &Settings) -> Result<(), String> {
    let parent = path.parent().ok_or("Pasta de configurações inválida.")?;
    fs::create_dir_all(parent).map_err(|_| "Não foi possível criar a pasta de configurações.")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Não foi possível salvar as configurações.")?;
    serde_json::to_writer(&mut file, settings)
        .map_err(|_| "Não foi possível salvar as configurações.")?;
    file.flush()
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "Não foi possível salvar as configurações.")?;
    file.persist(path)
        .map_err(|_| "Não foi possível salvar as configurações.")?;
    Ok(())
}

fn transform(data: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let len = u32::try_from(data.len()).map_err(|_| "Código de acesso inválido.")?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: len,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        let result = if protect {
            CryptProtectData(
                &input,
                PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        result.map_err(|_| "Não foi possível proteger ou recuperar o código de acesso.")?;
        let bytes = if output.cbData == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
        };
        if !output.pbData.is_null() {
            let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        }
        Ok(bytes)
    }
}

pub fn protect(code: &str) -> Result<Vec<u8>, String> {
    transform(code.as_bytes(), true)
}
pub fn unprotect(code: &[u8]) -> Result<String, String> {
    String::from_utf8(transform(code, false)?)
        .map_err(|_| "Código de acesso salvo inválido.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_code_and_settings_round_trip() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.json");
        let settings = Settings {
            name: "Ana".into(),
            protected_code: protect("segredo").unwrap(),
            ..Default::default()
        };
        write(&path, &settings).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("segredo"));
        let loaded = read(&path).unwrap();
        assert_eq!(loaded.name, "Ana");
        assert_eq!(unprotect(&loaded.protected_code).unwrap(), "segredo");
        write(&path, &loaded).unwrap();
    }

    #[test]
    fn imports_legacy_settings_without_the_access_code() {
        let folder = tempfile::tempdir().unwrap();
        let own = folder.path().join("getScreenShare-rust/settings.json");
        let legacy = folder.path().join("getScreenShare/native-settings.json");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(
            legacy,
            r#"{"name":"Ana","room":"jogo","server":"https://tokens.example","share_audio":true,"fps_mode":1,"show_stats":true}"#,
        )
        .unwrap();

        let imported = read(&own).unwrap();
        assert_eq!(imported.name, "Ana");
        assert_eq!(imported.room, "jogo");
        assert_eq!(imported.server, "https://tokens.example");
        assert!(imported.share_audio);
        assert_eq!(imported.fps_mode, 1);
        assert!(imported.show_stats);
        assert!(imported.protected_code.is_empty());
        assert!(!own.exists());
    }
}
