#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

mod instance;
#[cfg(windows)]
#[path = "../../egui-probe/src/settings.rs"]
mod legacy_settings;
mod updater;

#[cfg(windows)]
#[path = "../../egui-probe/src/discord.rs"]
mod discord;
#[cfg(windows)]
#[path = "../../egui-probe/src/wasapi.rs"]
mod wasapi;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenRequest {
    room: String,
    identity: String,
    name: String,
    server_url: String,
    access_code: String,
}

#[derive(Deserialize, Serialize)]
struct TokenResponse {
    token: String,
    url: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<String>,
}

#[cfg(not(windows))]
#[derive(Deserialize)]
struct LegacySettings {
    name: Option<String>,
    room: Option<String>,
    server: Option<String>,
    share_audio: Option<bool>,
    fps_mode: Option<u8>,
    show_stats: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientSettings {
    name: String,
    room: String,
    server_url: String,
    access_code: String,
    share_audio: bool,
    quality: String,
    camera_device_id: String,
    show_stats: bool,
}

#[derive(Serialize)]
struct Source {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    thumbnail: &'static str,
}

static TRAY_AVAILABLE: AtomicBool = AtomicBool::new(false);
static EXITING: AtomicBool = AtomicBool::new(false);

struct UpdateState {
    available: Option<updater::Available>,
    installer: Option<std::path::PathBuf>,
    status: String,
    version: Option<String>,
    error: Option<String>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            available: None,
            installer: None,
            status: if cfg!(windows) { "idle" } else { "unsupported" }.into(),
            version: None,
            error: None,
        }
    }
}

fn updater_snapshot(state: &UpdateState, current_version: String) -> serde_json::Value {
    serde_json::json!({
        "status": state.status,
        "currentVersion": current_version,
        "version": state.version,
        "percent": if state.status == "downloaded" { 100 } else { 0 },
        "error": state.error,
    })
}

fn emit_updater(app: &AppHandle, state: &UpdateState) {
    let snapshot = updater_snapshot(state, app.package_info().version.to_string());
    let _ = app.emit("updater-status", snapshot);
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
        WindowEvent,
    };

    let activation_app = app.handle().clone();
    instance::listen_for_activation(move || show_window(&activation_app));

    if let Ok(icon) = tray_image() {
        if let (Ok(show), Ok(quit)) = (
            MenuItem::with_id(app, "show", "Abrir getScreenShare", true, None::<&str>),
            MenuItem::with_id(app, "quit", "Sair", true, None::<&str>),
        ) {
            if let Ok(menu) = Menu::with_items(app, &[&show, &quit]) {
                if TrayIconBuilder::new()
                    .icon(icon)
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "show" => show_window(app),
                        "quit" => {
                            EXITING.store(true, Ordering::Release);
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if matches!(
                            event,
                            TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            }
                        ) {
                            show_window(tray.app_handle());
                        }
                    })
                    .build(app)
                    .is_ok()
                {
                    TRAY_AVAILABLE.store(true, Ordering::Release);
                }
            }
        }
    }

    if let Some(window) = app.get_webview_window("main") {
        let tray_window = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if TRAY_AVAILABLE.load(Ordering::Acquire) && !EXITING.load(Ordering::Acquire) {
                    api.prevent_close();
                    let _ = tray_window.hide();
                }
            }
        });
    }
    if cfg!(windows) && !cfg!(debug_assertions) {
        let handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(6)).await;
            let result = updater::check().await;
            let state = handle.state::<std::sync::Mutex<UpdateState>>();
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match result {
                Ok(Some(available)) => {
                    state.version = Some(available.version.clone());
                    state.available = Some(available);
                    state.status = "available".into();
                }
                Ok(None) => state.status = "not-available".into(),
                Err(error) => {
                    state.status = "error".into();
                    state.error = Some(error);
                }
            }
            emit_updater(&handle, &state);
        });
    }
    Ok(())
}

fn tray_image() -> Result<tauri::image::Image<'static>, Box<dyn std::error::Error>> {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../icons/icon.png")));
    let mut reader = decoder.read_info()?;
    let mut data = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut data)?;
    data.truncate(frame.buffer_size());
    Ok(tauri::image::Image::new_owned(
        data,
        frame.width,
        frame.height,
    ))
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn server_base(raw: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| {
        "A URL do servidor de tokens é inválida. Exemplo: https://tokens.seudominio.com".to_string()
    })?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("A URL do servidor de tokens deve começar com http:// ou https://".into());
    }
    Ok(format!(
        "{}{}",
        url.origin().ascii_serialization(),
        url.path().trim_end_matches('/')
    ))
}

fn valid_room(room: &str) -> bool {
    !room.is_empty()
        && room.chars().count() <= 64
        && room
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | ' '))
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| "Não foi possível iniciar a conexão com o servidor.".into())
}

#[tauri::command]
async fn get_token(params: TokenRequest) -> Result<TokenResponse, String> {
    if !valid_room(&params.room) {
        return Err(
            "Nome de sala inválido. Use letras, números, espaço, - e _ (até 64 caracteres).".into(),
        );
    }
    if params.identity.is_empty() || params.identity.chars().count() > 64 {
        return Err("Identidade inválida.".into());
    }
    if params.name.chars().count() > 40 {
        return Err("Nome inválido (máximo de 40 caracteres).".into());
    }
    let base = server_base(&params.server_url)?;
    let response = client()?
        .post(format!("{base}/token"))
        .json(&serde_json::json!({
            "room": params.room,
            "identity": params.identity,
            "name": params.name,
            "accessCode": params.access_code,
        }))
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                "O servidor de tokens demorou demais para responder.".to_string()
            } else {
                "Não foi possível falar com o servidor de tokens. Confira a URL e a conexão."
                    .to_string()
            }
        })?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Código de acesso incorreto.".into());
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("Muitas tentativas seguidas. Espere um minuto e tente de novo.".into());
    }
    if !status.is_success() {
        let message = response
            .json::<ErrorResponse>()
            .await
            .ok()
            .and_then(|data| data.error)
            .unwrap_or_else(|| {
                format!(
                    "O servidor de tokens respondeu com erro {}.",
                    status.as_u16()
                )
            });
        return Err(message);
    }
    let result = response
        .json::<TokenResponse>()
        .await
        .map_err(|_| "Resposta inesperada do servidor de tokens.".to_string())?;
    if result.token.is_empty() || result.url.is_empty() {
        return Err("Resposta inesperada do servidor de tokens.".into());
    }
    Ok(result)
}

#[tauri::command]
async fn check_server(server_url: String) -> Result<bool, String> {
    let base = server_base(&server_url)?;
    let response = client()?
        .get(format!("{base}/health"))
        .send()
        .await
        .map_err(|_| {
            "Não foi possível falar com o servidor de tokens. Confira a URL e a conexão."
                .to_string()
        })?;
    if !response.status().is_success() {
        return Err(format!(
            "O servidor respondeu com erro {}.",
            response.status().as_u16()
        ));
    }
    Ok(true)
}

#[tauri::command]
fn load_legacy_settings() -> Result<Option<serde_json::Value>, String> {
    #[cfg(windows)]
    {
        let path = legacy_settings::path()?;
        let saved = legacy_settings::read(&path)?;
        let access_code = legacy_settings::unprotect(&saved.protected_code).unwrap_or_default();
        return Ok(Some(serde_json::json!({
            "name": saved.name,
            "room": saved.room,
            "serverUrl": saved.server,
            "accessCode": access_code,
            "shareAudio": saved.share_audio,
            "quality": if saved.fps_mode == 1 { "1080p60" } else { "1080p30" },
            "cameraDeviceId": saved.camera_device_id,
            "showStats": saved.show_stats,
        })));
    }
    #[cfg(not(windows))]
    {
        let Some(app_data) = std::env::var_os("APPDATA") else {
            return Ok(None);
        };
        let path = std::path::PathBuf::from(app_data)
            .join("getscreenshare")
            .join("native-settings.json");
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Não foi possível importar as configurações anteriores.".into()),
        };
        let saved: LegacySettings = serde_json::from_slice(&bytes)
            .map_err(|_| "As configurações anteriores estão inválidas.".to_string())?;
        let quality = if saved.fps_mode.unwrap_or(0) == 1 {
            "1080p60"
        } else {
            "1080p30"
        };
        Ok(Some(serde_json::json!({
            "name": saved.name.unwrap_or_default(),
            "room": saved.room.unwrap_or_default(),
            "serverUrl": saved.server.unwrap_or_default(),
            "shareAudio": saved.share_audio.unwrap_or(false),
            "quality": quality,
            "showStats": saved.show_stats.unwrap_or(true),
        })))
    }
}

#[tauri::command]
fn save_native_settings(settings: ClientSettings) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let path = legacy_settings::path()?;
        legacy_settings::write(
            &path,
            &legacy_settings::Settings {
                name: settings.name,
                room: settings.room,
                server: settings.server_url,
                protected_code: legacy_settings::protect(&settings.access_code)?,
                share_audio: settings.share_audio,
                fps_mode: u8::from(settings.quality.ends_with("60")),
                camera_device_id: settings.camera_device_id,
                show_stats: settings.show_stats,
            },
        )?;
        return Ok(true);
    }
    #[cfg(not(windows))]
    {
        let _ = settings;
        Ok(false)
    }
}

#[tauri::command]
fn list_sources() -> Vec<Source> {
    const THUMBNAIL: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 320 180'%3E%3Crect width='320' height='180' rx='12' fill='%2336343b'/%3E%3Crect x='110' y='48' width='100' height='68' rx='8' fill='none' stroke='%23d0bcff' stroke-width='5'/%3E%3Cpath d='M145 132h30m-15-16v16' stroke='%23d0bcff' stroke-width='5' stroke-linecap='round'/%3E%3C/svg%3E";
    vec![
        Source {
            id: "tauri-system-picker-screen",
            name: "Escolher tela ou janela pelo sistema",
            kind: "screen",
            thumbnail: THUMBNAIL,
        },
        Source {
            id: "tauri-system-picker-window",
            name: "Escolher janela pelo sistema",
            kind: "window",
            thumbnail: THUMBNAIL,
        },
    ]
}

#[tauri::command]
fn select_source(source_id: String) -> bool {
    matches!(
        source_id.as_str(),
        "tauri-system-picker-screen" | "tauri-system-picker-window"
    )
}

#[tauri::command]
async fn start_excluded_audio(app: AppHandle) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let pid = tauri::async_runtime::spawn_blocking(discord::discord_pid)
            .await
            .map_err(|_| {
                "Não foi possível identificar o Discord; áudio desativado.".to_string()
            })??;
        let started = tauri::async_runtime::spawn_blocking(move || {
            wasapi::start(
                pid,
                Arc::new(move |chunk| {
                    let _ = app.emit("excluded-audio-chunk", chunk);
                }),
            )
        })
        .await
        .map_err(|_| "Não foi possível iniciar a captura de áudio.".to_string())?;
        if !started {
            return Err("Não foi possível excluir o Discord do áudio; áudio desativado.".into());
        }
        return Ok(true);
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("A captura de áudio do computador está disponível apenas no Windows.".into())
    }
}

#[tauri::command]
fn stop_excluded_audio() -> bool {
    #[cfg(windows)]
    wasapi::stop();
    true
}

#[tauri::command]
fn get_app_info(app: AppHandle) -> serde_json::Value {
    serde_json::json!({
        "version": app.package_info().version.to_string(),
        "platform": if cfg!(windows) { "win32" } else { std::env::consts::OS },
        "isWayland": std::env::var_os("WAYLAND_DISPLAY").is_some(),
        "usesSystemPicker": true,
    })
}

#[tauri::command]
fn get_gpu_status() -> serde_json::Value {
    serde_json::json!({ "hardwareAcceleration": true, "videoEncode": "unknown" })
}

#[tauri::command]
fn open_gpu_diagnostics() -> bool {
    false
}

#[tauri::command]
fn updater_get_status(
    app: AppHandle,
    state: State<'_, std::sync::Mutex<UpdateState>>,
) -> serde_json::Value {
    let state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cfg!(debug_assertions) {
        return serde_json::json!({
            "status": "dev",
            "currentVersion": app.package_info().version.to_string(),
            "version": null,
            "percent": 0,
            "error": null,
        });
    }
    updater_snapshot(&state, app.package_info().version.to_string())
}

#[tauri::command]
async fn updater_check(
    app: AppHandle,
    state: State<'_, std::sync::Mutex<UpdateState>>,
) -> Result<serde_json::Value, String> {
    if !cfg!(windows) {
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.status = "unsupported".into();
        emit_updater(&app, &state);
        return Ok(updater_snapshot(
            &state,
            app.package_info().version.to_string(),
        ));
    }
    if cfg!(debug_assertions) {
        return Err("A verificação de atualização está disponível na versão instalada.".into());
    }
    {
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.status = "checking".into();
        state.error = None;
        emit_updater(&app, &state);
    }
    let result = updater::check().await;
    let mut state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match result {
        Ok(Some(available)) => {
            state.version = Some(available.version.clone());
            state.available = Some(available);
            state.installer = None;
            state.status = "available".into();
            state.error = None;
        }
        Ok(None) => {
            state.version = None;
            state.available = None;
            state.installer = None;
            state.status = "not-available".into();
            state.error = None;
        }
        Err(error) => {
            state.status = "error".into();
            state.error = Some(error);
        }
    }
    emit_updater(&app, &state);
    Ok(updater_snapshot(
        &state,
        app.package_info().version.to_string(),
    ))
}

#[tauri::command]
async fn updater_download(
    app: AppHandle,
    state: State<'_, std::sync::Mutex<UpdateState>>,
) -> Result<bool, String> {
    let available = {
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let update = state
            .available
            .clone()
            .ok_or("Não há atualização disponível para baixar.")?;
        state.status = "downloading".into();
        state.error = None;
        emit_updater(&app, &state);
        update
    };
    let progress_app = app.clone();
    let current_version = app.package_info().version.to_string();
    let version = available.version.clone();
    let on_progress = move |percent| {
        let _ = progress_app.emit(
            "updater-status",
            serde_json::json!({
                "status": "downloading",
                "currentVersion": current_version,
                "version": version,
                "percent": percent,
                "error": null,
            }),
        );
    };
    match updater::download(&available, on_progress).await {
        Ok(path) => {
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.installer = Some(path);
            state.status = "downloaded".into();
            emit_updater(&app, &state);
            Ok(true)
        }
        Err(error) => {
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.status = "error".into();
            state.error = Some(error.clone());
            emit_updater(&app, &state);
            Err(error)
        }
    }
}

#[tauri::command]
fn updater_install(
    app: AppHandle,
    state: State<'_, std::sync::Mutex<UpdateState>>,
) -> Result<bool, String> {
    let installer = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .installer
        .clone()
        .ok_or("Baixe a atualização antes de instalar.")?;
    updater::schedule_install(&installer)?;
    EXITING.store(true, Ordering::Release);
    app.exit(0);
    Ok(true)
}

fn main() {
    let _instance = match instance::acquire() {
        Ok(Some(instance)) => instance,
        Ok(None) => return,
        Err(error) => panic!("{error}"),
    };
    tauri::Builder::default()
        .manage(std::sync::Mutex::new(UpdateState::default()))
        .setup(setup_tray)
        .invoke_handler(tauri::generate_handler![
            get_token,
            check_server,
            load_legacy_settings,
            save_native_settings,
            list_sources,
            select_source,
            start_excluded_audio,
            stop_excluded_audio,
            get_app_info,
            get_gpu_status,
            open_gpu_diagnostics,
            updater_get_status,
            updater_check,
            updater_download,
            updater_install,
        ])
        .run(tauri::generate_context!())
        .expect("Falha ao iniciar getScreenShare");
}
