#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod camera;
mod connection;
#[cfg(windows)]
mod discord;
#[cfg(windows)]
mod instance;
mod media;
#[cfg(windows)]
mod send_audio;
#[cfg(windows)]
mod settings;
mod stats;
mod update;
mod video;
#[cfg(windows)]
mod wasapi;

use eframe::egui::{self, Color32, RichText};
use livekit::webrtc::video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution};
use livekit::{
    options::{TrackPublishOptions, VideoCodec, VideoEncoding},
    prelude::*,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
};
use tokio::sync::mpsc as async_mpsc;

enum Event {
    Connected,
    Disconnected,
    Error(String),
    Shared(media::Source),
    ShareSwitched(media::Source),
    ShareFailed,
    Stopped,
    Notice(String),
    VideoAdded(String, String, video::LatestFrame),
    VideoRemoved(String),
    AudioError(String),
    CameraError(String),
    Stats(stats::Snapshot),
    Cameras(Vec<camera::Device>),
    CameraStarted,
    CameraStopped,
}
enum Command {
    Share(media::Source, bool, u32),
    SwitchShare(media::Source, u32),
    RefreshCameras,
    StartCamera(String),
    StopCamera,
    Stop,
    CaptureFailed(u64),
    AudioFailed,
    CameraFailed,
    Leave,
}
enum UpdateEvent {
    Checked(Result<Option<update::Available>, String>),
    Downloaded(Result<std::path::PathBuf, String>),
}

fn main() -> eframe::Result {
    let _instance =
        match instance::acquire().map_err(|error| eframe::Error::AppCreation(Box::new(error)))? {
            Some(instance) => instance,
            None => return Ok(()),
        };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "getScreenShare — Windows",
        options,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "roboto".into(),
                std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                    "../assets/Roboto.ttf"
                ))),
            );
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "roboto".into());
            cc.egui_ctx.set_fonts(fonts);
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.item_spacing = egui::vec2(12.0, 12.0);
            style.spacing.button_padding = egui::vec2(18.0, 12.0);
            style.visuals.panel_fill = Color32::from_rgb(20, 18, 24);
            style.visuals.window_fill = Color32::from_rgb(33, 31, 38);
            style.visuals.extreme_bg_color = Color32::from_rgb(29, 27, 32);
            style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);
            style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(54, 52, 59);
            style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(74, 68, 83);
            style.visuals.widgets.active.bg_fill = Color32::from_rgb(80, 61, 117);
            style.visuals.selection.bg_fill = Color32::from_rgb(91, 72, 128);
            cc.egui_ctx.set_style(style);
            Ok(Box::new(App::load()))
        }),
    )
}

struct Video {
    label: String,
    frame: video::LatestFrame,
    texture: Option<egui::TextureHandle>,
    zoom: f32,
}

struct ScreenShare {
    id: u64,
    capture: media::Capture,
    track: LocalVideoTrack,
    source: media::Source,
}

#[derive(Default)]
struct App {
    name: String,
    room: String,
    server: String,
    code: String,
    status: String,
    settings_writable: bool,
    busy: bool,
    connected: bool,
    sharing: bool,
    share_busy: bool,
    switching_source: bool,
    active_source: Option<media::Source>,
    share_audio: bool,
    show_stats: bool,
    stats: stats::Snapshot,
    camera_device_id: String,
    cameras: Vec<camera::Device>,
    selected_camera: usize,
    camera_active: bool,
    camera_busy: bool,
    fps_mode: u8,
    cancelled: Arc<AtomicBool>,
    videos: HashMap<String, Video>,
    sources: Vec<media::Source>,
    selected: usize,
    events: Option<mpsc::Receiver<Event>>,
    update_events: Option<mpsc::Receiver<UpdateEvent>>,
    update_info: Option<update::Available>,
    update_path: Option<std::path::PathBuf>,
    update_busy: bool,
    update_status: String,
    commands: Option<async_mpsc::UnboundedSender<Command>>,
}

async fn capture_ready(capture: &mut media::Capture) -> Result<(), String> {
    let receiver = capture.take_ready();
    match tokio::task::spawn_blocking(move || {
        receiver.recv_timeout(std::time::Duration::from_secs(8))
    })
    .await
    {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(error))) => Err(error),
        _ => Err("A captura não recebeu o primeiro quadro.".into()),
    }
}

async fn publish_screen(
    room: &Room,
    source: media::Source,
    fps: u32,
    id: u64,
    cancelled: Arc<AtomicBool>,
    commands: async_mpsc::UnboundedSender<Command>,
) -> Result<ScreenShare, String> {
    let probe = source.clone();
    let (width, height) = tokio::task::spawn_blocking(move || media::dimensions(probe))
        .await
        .map_err(|_| "Não foi possível consultar a fonte selecionada.")??;
    let sink = NativeVideoSource::new(VideoResolution { width, height }, true);
    let track = LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(sink.clone()));
    let options = TrackPublishOptions {
        source: TrackSource::Screenshare,
        video_codec: VideoCodec::H264,
        video_encoding: Some(VideoEncoding {
            max_bitrate: media::screen_bitrate(width, height, fps),
            max_framerate: f64::from(fps),
        }),
        simulcast: false,
        ..Default::default()
    };
    room.local_participant()
        .publish_track(LocalTrack::Video(track.clone()), options)
        .await
        .map_err(|_| "Não foi possível publicar a tela.")?;
    let mut capture =
        media::Capture::start(source.clone(), sink.clone(), fps, cancelled, move || {
            let _ = commands.send(Command::CaptureFailed(id));
        });
    if let Err(error) = capture_ready(&mut capture).await {
        drop(capture);
        let _ = room.local_participant().unpublish_track(&track.sid()).await;
        return Err(error);
    }
    Ok(ScreenShare {
        id,
        capture,
        track,
        source,
    })
}

impl App {
    fn load() -> Self {
        let mut app = Self::default();
        match settings::path().and_then(|path| settings::read(&path)) {
            Ok(saved) => {
                app.name = saved.name;
                app.room = saved.room;
                app.server = saved.server;
                app.share_audio = saved.share_audio;
                app.show_stats = saved.show_stats;
                app.camera_device_id = saved.camera_device_id;
                app.fps_mode = saved.fps_mode.min(2);
                match settings::unprotect(&saved.protected_code) {
                    Ok(code) => {
                        app.code = code;
                        app.settings_writable = true;
                    }
                    Err(error) => app.status = error,
                }
            }
            Err(error) => app.status = error,
        }
        app
    }

    fn save_settings(&self) -> Result<(), String> {
        if !self.settings_writable {
            return Err("As configurações anteriores não foram alteradas.".into());
        }
        settings::write(
            &settings::path()?,
            &settings::Settings {
                name: self.name.clone(),
                room: self.room.clone(),
                server: self.server.clone(),
                protected_code: settings::protect(&self.code)?,
                share_audio: self.share_audio,
                fps_mode: self.fps_mode,
                camera_device_id: self.camera_device_id.clone(),
                show_stats: self.show_stats,
            },
        )
    }

    fn check_updates(&mut self, ctx: &egui::Context) {
        if self.update_busy {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.update_events = Some(receiver);
        self.update_busy = true;
        self.update_status = "Procurando atualizações…".into();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "Não foi possível iniciar o verificador.".to_owned())
                .and_then(|runtime| runtime.block_on(update::check()));
            let _ = sender.send(UpdateEvent::Checked(result));
            context.request_repaint();
        });
    }

    fn download_update(&mut self, ctx: &egui::Context) {
        let Some(update) = self.update_info.clone() else {
            return;
        };
        if self.update_busy {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.update_events = Some(receiver);
        self.update_busy = true;
        self.update_status = format!("Baixando versão {}…", update.version);
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "Não foi possível iniciar o download.".to_owned())
                .and_then(|runtime| runtime.block_on(update::download(update)));
            let _ = sender.send(UpdateEvent::Downloaded(result));
            context.request_repaint();
        });
    }

    fn update_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.separator();
        ui.label(RichText::new("Atualizações").size(17.0).strong());
        if ui
            .add_enabled(
                !self.update_busy,
                egui::Button::new("Verificar atualizações"),
            )
            .clicked()
        {
            self.check_updates(ctx);
        }
        if self.update_info.is_some() && self.update_path.is_none() {
            if ui
                .add_enabled(!self.update_busy, egui::Button::new("Baixar versão nova"))
                .clicked()
            {
                self.download_update(ctx);
            }
        }
        if let Some(path) = &self.update_path {
            if ui.button("Instalar e reiniciar").clicked() {
                match update::schedule_install(path) {
                    Ok(()) => {
                        instance::allow_exit();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => self.update_status = error,
                }
            }
        }
        if self.update_busy {
            ui.spinner();
        }
        if !self.update_status.is_empty() {
            ui.label(RichText::new(&self.update_status).size(12.0));
        }
    }

    fn join(&mut self, ctx: egui::Context) {
        let (tx, rx) = mpsc::channel();
        let (cmd_tx, mut cmd_rx) = async_mpsc::unbounded_channel();
        let thread_commands = cmd_tx.clone();
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        let (server, code, room, name) = (
            self.server.clone(),
            self.code.clone(),
            self.room.clone(),
            self.name.clone(),
        );
        self.busy = true;
        self.status = "Conectando...".into();
        self.events = Some(rx);
        self.commands = Some(cmd_tx);
        std::thread::spawn(move || {
            let notify = |event| {
                let _ = tx.send(event);
                ctx.request_repaint();
            };
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => {
                    notify(Event::Error("Não foi possível iniciar a conexão.".into()));
                    return;
                }
            };
            runtime.block_on(async {
                match connection::connect(&server, &code, &room, &name).await {
                    Ok((room, mut events)) => {
                        notify(Event::Connected);
                        let mut sharing: Option<ScreenShare> = None;
                        let mut camera_share: Option<(camera::Capture, LocalVideoTrack)> = None;
                        let mut audio_share: Option<send_audio::AudioShare> = None;
                        let mut next_capture_id = 0u64;
                        let mut video_tasks = HashMap::new();
                        let mut audio_tracks = HashMap::new();
                        let mut stats_timer = tokio::time::interval(std::time::Duration::from_secs(1));
                        stats_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        let mut previous_stats = HashMap::new();
                        loop {
                            tokio::select! {
                                command = cmd_rx.recv() => match command {
                                    Some(Command::Share(source, with_audio, fps)) if sharing.is_none() => {
                                        next_capture_id = next_capture_id.wrapping_add(1);
                                        let id = next_capture_id;
                                        match publish_screen(&room, source, fps, id, cancelled.clone(), thread_commands.clone()).await {
                                            Ok(screen) => {
                                                let active_source = screen.source.clone();
                                                sharing = Some(screen);
                                                let audio_error = if with_audio {
                                                    let failure_commands = thread_commands.clone();
                                                    match send_audio::start(&room, cancelled.clone(), move || { let _ = failure_commands.send(Command::AudioFailed); }).await {
                                                        Ok(audio) => { audio_share = Some(audio); None }
                                                        Err(error) => Some(error),
                                                    }
                                                } else { None };
                                                notify(Event::Shared(active_source));
                                                if let Some(error) = audio_error { notify(Event::AudioError(error)); }
                                            }
                                            Err(error) => notify(Event::Notice(error)),
                                        }
                                    }
                                    Some(Command::SwitchShare(source, fps)) if sharing.is_some() => {
                                        next_capture_id = next_capture_id.wrapping_add(1);
                                        let id = next_capture_id;
                                        match publish_screen(&room, source, fps, id, cancelled.clone(), thread_commands.clone()).await {
                                            Ok(candidate) => {
                                                let old = sharing.replace(candidate).unwrap();
                                                drop(old.capture);
                                                if room.local_participant().unpublish_track(&old.track.sid()).await.is_err() {
                                                    notify(Event::Notice("A nova fonte está ativa, mas a publicação anterior não foi removida.".into()));
                                                }
                                                notify(Event::ShareSwitched(sharing.as_ref().unwrap().source.clone()));
                                            }
                                            Err(error) => notify(Event::Notice(error)),
                                        }
                                    }
                                    Some(Command::RefreshCameras) => {
                                        let sender = tx.clone();
                                        let context = ctx.clone();
                                        tokio::task::spawn_blocking(move || {
                                            let event = match camera::devices() {
                                                Ok(devices) => Event::Cameras(devices),
                                                Err(error) => Event::CameraError(error),
                                            };
                                            let _ = sender.send(event);
                                            context.request_repaint();
                                        });
                                    }
                                    Some(Command::StartCamera(id)) if camera_share.is_none() => {
                                        let failed_commands = thread_commands.clone();
                                        let mut capture = camera::Capture::start(id.clone(), move || { let _ = failed_commands.send(Command::CameraFailed); });
                                        let ready = capture.take_ready();
                                        let result = tokio::task::spawn_blocking(move || ready.recv_timeout(std::time::Duration::from_secs(10))).await;
                                        let (width, height) = match result {
                                            Ok(Ok(Ok(size))) => size,
                                            Ok(Ok(Err(error))) => { notify(Event::CameraError(error)); continue; }
                                            _ => { notify(Event::CameraError("A câmera demorou para responder.".into())); continue; }
                                        };
                                        let sink = NativeVideoSource::new(VideoResolution { width, height }, false);
                                        let track = LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(sink.clone()));
                                        let options = TrackPublishOptions { source: TrackSource::Camera, video_codec: VideoCodec::H264, video_encoding: Some(VideoEncoding { max_bitrate: 2_000_000, max_framerate: 30.0 }), simulcast: false, ..Default::default() };
                                        if room.local_participant().publish_track(LocalTrack::Video(track.clone()), options).await.is_err() {
                                            notify(Event::CameraError("Não foi possível publicar a câmera.".into()));
                                            continue;
                                        }
                                        if let Err(error) = capture.activate(sink) {
                                            let _ = room.local_participant().unpublish_track(&track.sid()).await;
                                            notify(Event::CameraError(error));
                                            continue;
                                        }
                                        camera_share = Some((capture, track));
                                        notify(Event::CameraStarted);
                                    }
                                    Some(Command::StopCamera | Command::CameraFailed) => {
                                        let failed = matches!(command, Some(Command::CameraFailed));
                                        if let Some((capture, track)) = camera_share.take() {
                                            drop(capture);
                                            let _ = room.local_participant().unpublish_track(&track.sid()).await;
                                            notify(if failed { Event::CameraError("A câmera foi interrompida pelo sistema.".into()) } else { Event::CameraStopped });
                                        }
                                    }
                                    Some(Command::Stop) => {
                                        if let Some(share) = sharing.take() {
                                            if let Some(audio) = audio_share.take() {
                                                let sid = audio.track.sid();
                                                drop(audio);
                                                let _ = room.local_participant().unpublish_track(&sid).await;
                                            }
                                            drop(share.capture);
                                            if room.local_participant().unpublish_track(&share.track.sid()).await.is_err() {
                                                notify(Event::Notice("A captura foi encerrada, mas não foi possível remover a publicação.".into()));
                                            } else { notify(Event::Stopped); }
                                        }
                                    }
                                    Some(Command::CaptureFailed(id)) if sharing.as_ref().is_some_and(|share| share.id == id) => {
                                        if let Some(share) = sharing.take() {
                                            if let Some(audio) = audio_share.take() {
                                                let sid = audio.track.sid();
                                                drop(audio);
                                                let _ = room.local_participant().unpublish_track(&sid).await;
                                            }
                                            drop(share.capture);
                                            let _ = room.local_participant().unpublish_track(&share.track.sid()).await;
                                            notify(Event::ShareFailed);
                                        }
                                    }
                                    Some(Command::AudioFailed) => {
                                        if let Some(audio) = audio_share.take() {
                                            let sid = audio.track.sid();
                                            drop(audio);
                                            let _ = room.local_participant().unpublish_track(&sid).await;
                                            notify(Event::AudioError("A captura de áudio foi interrompida; a tela continua compartilhada.".into()));
                                        }
                                    }
                                    Some(Command::Leave) | None => { drop(audio_share.take()); drop(sharing.take()); drop(camera_share.take()); let _ = room.close().await; break; }
                                    _ => {}
                                },
                                _ = stats_timer.tick() => {
                                    if let Ok(snapshot) = room.get_stats().await {
                                        notify(Event::Stats(stats::collect(&snapshot, &mut previous_stats)));
                                    }
                                },
                                event = events.recv() => {
                                    match event {
                                        Some(RoomEvent::TrackSubscribed { track: RemoteTrack::Audio(track), .. }) => {
                                            let id = track.sid().to_string();
                                            let error_sender = tx.clone();
                                            let error_ctx = ctx.clone();
                                            audio_tracks.insert(id, audio::Audio::start(track, move |error| {
                                                let _ = error_sender.send(Event::AudioError(error));
                                                error_ctx.request_repaint();
                                            }));
                                        }
                                        Some(RoomEvent::TrackUnsubscribed { track: RemoteTrack::Audio(track), .. }) => {
                                            audio_tracks.remove(&track.sid().to_string());
                                        }
                                        Some(RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), participant, .. }) => {
                                            let id = track.sid().to_string();
                                            let latest = Arc::new(Mutex::new(None));
                                            notify(Event::VideoAdded(id.clone(), participant.name(), latest.clone()));
                                            if let Some(task) = video_tasks.insert(id, tokio::spawn(video::receive(track, latest, ctx.clone()))) {
                                                tokio::task::JoinHandle::<()>::abort(&task);
                                            }
                                        }
                                        Some(RoomEvent::TrackUnsubscribed { track: RemoteTrack::Video(track), .. }) => {
                                            let id = track.sid().to_string();
                                            if let Some(task) = video_tasks.remove(&id) { task.abort(); }
                                            notify(Event::VideoRemoved(id));
                                        }
                                        None | Some(RoomEvent::Disconnected { .. }) => break,
                                        _ => {}
                                    }
                                }
                            }
                        }
                        drop(audio_share);
                        drop(sharing);
                        drop(camera_share);
                        drop(audio_tracks);
                        for (_, task) in video_tasks { task.abort(); }
                        notify(Event::Disconnected);
                    }
                    Err(error) => notify(Event::Error(error)),
                }
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let _ = instance::install_tray();
        if !cfg!(debug_assertions) && self.update_status.is_empty() {
            self.check_updates(ctx);
        }
        if let Some(events) = &self.update_events {
            while let Ok(event) = events.try_recv() {
                self.update_busy = false;
                match event {
                    UpdateEvent::Checked(Ok(Some(update))) => {
                        self.update_status = format!("Versão {} disponível.", update.version);
                        self.update_info = Some(update);
                        self.update_path = None;
                    }
                    UpdateEvent::Checked(Ok(None)) => {
                        self.update_status = "Você está na versão mais recente.".into();
                        self.update_info = None;
                    }
                    UpdateEvent::Checked(Err(error)) | UpdateEvent::Downloaded(Err(error)) => {
                        self.update_status = error;
                    }
                    UpdateEvent::Downloaded(Ok(path)) => {
                        self.update_status = "Download pronto para instalar.".into();
                        self.update_path = Some(path);
                    }
                }
            }
        }
        if self.update_busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if let Some(events) = &self.events {
            while let Ok(event) = events.try_recv() {
                match event {
                    Event::Connected => {
                        self.busy = false;
                        self.connected = true;
                        self.status = "Conectado à sala.".into();
                    }
                    Event::Disconnected => {
                        self.busy = false;
                        self.connected = false;
                        self.sharing = false;
                        self.share_busy = false;
                        self.switching_source = false;
                        self.active_source = None;
                        self.camera_active = false;
                        self.camera_busy = false;
                        self.status = "Sala encerrada.".into();
                        self.videos.clear();
                    }
                    Event::Error(error) => {
                        self.busy = false;
                        self.connected = false;
                        self.status = error;
                    }
                    Event::Shared(source) => {
                        self.sharing = true;
                        self.share_busy = false;
                        self.active_source = Some(source.clone());
                        self.selected = self
                            .sources
                            .iter()
                            .position(|item| item.id == source.id && item.kind == source.kind)
                            .unwrap_or(self.selected);
                        self.status = "Tela compartilhada.".into();
                    }
                    Event::ShareSwitched(source) => {
                        self.active_source = Some(source.clone());
                        self.selected = self
                            .sources
                            .iter()
                            .position(|item| item.id == source.id && item.kind == source.kind)
                            .unwrap_or(self.selected);
                        self.switching_source = false;
                        self.share_busy = false;
                        self.status = format!("Transmitindo: {}.", source.title);
                    }
                    Event::ShareFailed => {
                        self.sharing = false;
                        self.share_busy = false;
                        self.switching_source = false;
                        self.active_source = None;
                        self.status = "A captura foi interrompida pelo sistema.".into();
                    }
                    Event::Stopped => {
                        self.sharing = false;
                        self.share_busy = false;
                        self.switching_source = false;
                        self.active_source = None;
                        self.status = "Compartilhamento encerrado.".into();
                    }
                    Event::AudioError(error) => {
                        self.status = error;
                    }
                    Event::Cameras(cameras) => {
                        self.cameras = cameras;
                        self.camera_busy = false;
                        self.selected_camera = self
                            .cameras
                            .iter()
                            .position(|camera| camera.id == self.camera_device_id)
                            .unwrap_or(0);
                        self.status = if self.cameras.is_empty() {
                            "Nenhuma câmera encontrada.".into()
                        } else {
                            String::new()
                        };
                    }
                    Event::CameraStarted => {
                        self.camera_active = true;
                        self.camera_busy = false;
                        self.status = "Câmera ligada.".into();
                    }
                    Event::CameraStopped => {
                        self.camera_active = false;
                        self.camera_busy = false;
                        self.status = "Câmera desligada.".into();
                    }
                    Event::CameraError(error) => {
                        self.camera_active = false;
                        self.camera_busy = false;
                        self.status = error;
                    }
                    Event::Stats(snapshot) => self.stats = snapshot,
                    Event::VideoAdded(id, label, frame) => {
                        self.videos.insert(
                            id,
                            Video {
                                label,
                                frame,
                                texture: None,
                                zoom: 1.0,
                            },
                        );
                    }
                    Event::VideoRemoved(id) => {
                        self.videos.remove(&id);
                    }
                    Event::Notice(message) => {
                        self.share_busy = false;
                        self.status = message;
                    }
                }
            }
        }
        egui::TopBottomPanel::top("header")
            .exact_height(68.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(20, 18, 24))
                    .inner_margin(egui::Margin::symmetric(28, 16)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 11.0, Color32::from_rgb(208, 188, 255));
                    let screen = rect.shrink2(egui::vec2(8.0, 10.0));
                    ui.painter().rect_stroke(
                        screen,
                        2.0,
                        egui::Stroke::new(1.6_f32, Color32::from_rgb(56, 30, 114)),
                        egui::StrokeKind::Inside,
                    );
                    ui.add_space(3.0);
                    ui.label(
                        RichText::new("getScreenShare")
                            .size(19.0)
                            .strong()
                            .color(Color32::from_rgb(230, 224, 233)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if self.connected {
                            "NA SALA"
                        } else {
                            "COMPARTILHAMENTO DE TELA"
                        };
                        ui.label(
                            RichText::new(label)
                                .size(11.0)
                                .color(Color32::from_rgb(166, 160, 174)),
                        );
                    });
                });
            });
        if self.connected {
            egui::SidePanel::left("room_controls")
                .exact_width(310.0)
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_rgb(29, 27, 32))
                        .inner_margin(24),
                )
                .show(ctx, |ui| self.room_controls(ui, ctx));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(20, 18, 24))
                    .inner_margin(32),
            )
            .show(ctx, |ui| {
                if self.connected {
                    self.room_stage(ui);
                } else {
                    self.lobby(ui, ctx);
                }
            });
    }
}

impl App {
    fn lobby(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(35.0);
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    ui.add_space(30.0);
                    ui.label(
                        RichText::new("FEITO PARA ESTAR JUNTO")
                            .size(11.0)
                            .strong()
                            .color(Color32::from_rgb(208, 188, 255)),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Sua tela.\nTodo mundo\nmais perto.")
                            .size(40.0)
                            .strong()
                            .color(Color32::from_rgb(230, 224, 233)),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Compartilhe jogos, ideias e momentos em uma sala só sua.")
                            .size(16.0)
                            .color(Color32::from_rgb(202, 196, 208)),
                    );
                    ui.add_space(34.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(33, 31, 38))
                        .corner_radius(24)
                        .inner_margin(22)
                        .show(ui, |ui| {
                            ui.set_min_width((ui.available_width() - 44.0).max(180.0));
                            ui.label(RichText::new("Compartilhar é simples").size(16.0).strong());
                            ui.add_space(5.0);
                            ui.label(
                                RichText::new(
                                    "Entre na mesma sala que seus amigos e escolha o que mostrar.",
                                )
                                .size(14.0)
                                .color(Color32::from_rgb(202, 196, 208)),
                            );
                        });
                });
                columns[1].vertical(|ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(43, 41, 48))
                        .corner_radius(28)
                        .inner_margin(28)
                        .show(ui, |ui| {
                            ui.set_min_width((ui.available_width() - 56.0).max(240.0));
                            ui.label(RichText::new("Entre na sala").size(26.0).strong());
                            ui.label(
                                RichText::new("Configure sua conexão para começar.")
                                    .size(14.0)
                                    .color(Color32::from_rgb(202, 196, 208)),
                            );
                            ui.add_space(14.0);
                            ui.add_enabled_ui(!self.busy, |ui| {
                                field(
                                    ui,
                                    "Servidor de tokens",
                                    &mut self.server,
                                    "https://...",
                                    false,
                                );
                                field(ui, "Código de acesso", &mut self.code, "Seu código", true);
                                field(
                                    ui,
                                    "Seu nome",
                                    &mut self.name,
                                    "Como podemos chamar você?",
                                    false,
                                );
                                field(ui, "Sala", &mut self.room, "Nome da sala", false);
                                ui.add_space(8.0);
                                let button = egui::Button::new(
                                    RichText::new("Entrar na sala")
                                        .size(15.0)
                                        .strong()
                                        .color(Color32::from_rgb(56, 30, 114)),
                                )
                                .fill(Color32::from_rgb(208, 188, 255))
                                .corner_radius(24);
                                if ui.add_sized([ui.available_width(), 48.0], button).clicked() {
                                    let result = self.save_settings();
                                    self.join(ctx.clone());
                                    if let Err(error) = result {
                                        self.status = format!("Conectando... {error}");
                                    }
                                }
                            });
                            if !self.status.is_empty() {
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(&self.status)
                                        .color(Color32::from_rgb(255, 180, 171)),
                                );
                            }
                            self.update_controls(ui, ctx);
                        });
                });
            });
        });
    }

    fn room_controls(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.label(RichText::new("Sua sala").size(22.0).strong());
        ui.label(
            RichText::new(&self.room)
                .size(14.0)
                .color(Color32::from_rgb(208, 188, 255)),
        );
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(10.0);
        ui.label(RichText::new("Compartilhar").size(17.0).strong());
        ui.label(
            RichText::new("Escolha uma tela ou janela para mostrar.")
                .size(13.0)
                .color(Color32::from_rgb(202, 196, 208)),
        );
        ui.add_space(5.0);
        if !self.sharing && !self.share_busy {
            if ui
                .add_sized(
                    [ui.available_width(), 40.0],
                    egui::Button::new("Escolher fonte").corner_radius(20),
                )
                .clicked()
            {
                self.sources = media::sources();
                self.selected = 0;
                if self.sources.is_empty() {
                    self.status = "Nenhuma tela ou janela encontrada.".into();
                }
            }
            if !self.sources.is_empty() {
                ui.label(
                    RichText::new("FONTE")
                        .size(11.0)
                        .strong()
                        .color(Color32::from_rgb(202, 196, 208)),
                );
                egui::ComboBox::from_id_salt("source")
                    .width(ui.available_width() - 24.0)
                    .selected_text(&self.sources[self.selected].title)
                    .show_ui(ui, |ui| {
                        for (i, source) in self.sources.iter().enumerate() {
                            ui.selectable_value(&mut self.selected, i, &source.title);
                        }
                    });
                ui.label(
                    RichText::new("QUALIDADE")
                        .size(11.0)
                        .strong()
                        .color(Color32::from_rgb(202, 196, 208)),
                );
                egui::ComboBox::from_id_salt("quality")
                    .width(ui.available_width() - 24.0)
                    .selected_text(match self.fps_mode {
                        1 => "Fonte @ 60 FPS",
                        2 => "Fonte @ Fonte",
                        _ => "Fonte @ 30 FPS",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.fps_mode, 0, "Fonte @ 30 FPS");
                        ui.selectable_value(&mut self.fps_mode, 1, "Fonte @ 60 FPS");
                        ui.selectable_value(&mut self.fps_mode, 2, "Fonte @ Fonte");
                    });
                ui.checkbox(&mut self.share_audio, "Compartilhar áudio sem Discord");
                let button = egui::Button::new(
                    RichText::new("Compartilhar agora")
                        .strong()
                        .color(Color32::from_rgb(56, 30, 114)),
                )
                .fill(Color32::from_rgb(208, 188, 255))
                .corner_radius(22);
                if ui.add_sized([ui.available_width(), 44.0], button).clicked() {
                    if let Err(error) = self.save_settings() {
                        self.status = error;
                    }
                    if let Some(commands) = &self.commands {
                        let source = self.sources[self.selected].clone();
                        let fps = match self.fps_mode {
                            1 => Ok(60),
                            2 => media::refresh_rate(&source),
                            _ => Ok(30),
                        };
                        match fps {
                            Ok(fps) => {
                                let _ =
                                    commands.send(Command::Share(source, self.share_audio, fps));
                                self.share_busy = true;
                            }
                            Err(error) => self.status = error,
                        }
                    }
                }
            }
        }
        if self.sharing && !self.share_busy && !self.switching_source {
            if ui
                .add_sized(
                    [ui.available_width(), 40.0],
                    egui::Button::new("Trocar tela ou janela").corner_radius(20),
                )
                .clicked()
            {
                self.sources = media::sources();
                self.selected = self
                    .active_source
                    .as_ref()
                    .and_then(|active| {
                        self.sources
                            .iter()
                            .position(|source| source.id == active.id && source.kind == active.kind)
                    })
                    .unwrap_or(0);
                self.switching_source = !self.sources.is_empty();
                if self.sources.is_empty() {
                    self.status = "Nenhuma outra fonte encontrada.".into();
                }
            }
        }
        if self.sharing && self.switching_source && !self.sources.is_empty() {
            egui::ComboBox::from_id_salt("switch-source")
                .width(ui.available_width() - 24.0)
                .selected_text(&self.sources[self.selected].title)
                .show_ui(ui, |ui| {
                    for (index, source) in self.sources.iter().enumerate() {
                        ui.selectable_value(&mut self.selected, index, &source.title);
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Cancelar").clicked() {
                    self.switching_source = false;
                }
                if ui.button("Trocar agora").clicked() {
                    let source = self.sources[self.selected].clone();
                    let fps = match self.fps_mode {
                        1 => Ok(60),
                        2 => media::refresh_rate(&source),
                        _ => Ok(30),
                    };
                    match fps {
                        Ok(fps) => {
                            if let Some(commands) = &self.commands {
                                let _ = commands.send(Command::SwitchShare(source, fps));
                                self.share_busy = true;
                            }
                        }
                        Err(error) => self.status = error,
                    }
                }
            });
        }
        if self.sharing
            && !self.share_busy
            && ui
                .add_sized(
                    [ui.available_width(), 42.0],
                    egui::Button::new("Parar compartilhamento").corner_radius(21),
                )
                .clicked()
        {
            if let Some(commands) = &self.commands {
                let _ = commands.send(Command::Stop);
                self.share_busy = true;
            }
        }
        if self.share_busy {
            ui.spinner();
        }
        ui.separator();
        ui.label(RichText::new("Câmera").size(17.0).strong());
        if self.camera_active {
            if ui
                .add_sized(
                    [ui.available_width(), 40.0],
                    egui::Button::new("Desligar câmera").corner_radius(20),
                )
                .clicked()
            {
                if let Some(commands) = &self.commands {
                    let _ = commands.send(Command::StopCamera);
                    self.camera_busy = true;
                }
            }
        } else if self.cameras.is_empty() {
            if ui
                .add_sized(
                    [ui.available_width(), 40.0],
                    egui::Button::new(if self.camera_busy {
                        "Procurando câmeras…"
                    } else {
                        "Detectar câmera"
                    })
                    .corner_radius(20),
                )
                .clicked()
                && !self.camera_busy
            {
                if let Some(commands) = &self.commands {
                    let _ = commands.send(Command::RefreshCameras);
                    self.camera_busy = true;
                }
            }
        } else {
            let selected = self
                .cameras
                .get(self.selected_camera)
                .map(|camera| camera.name.as_str())
                .unwrap_or("Câmera");
            egui::ComboBox::from_id_salt("camera")
                .width(ui.available_width() - 24.0)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (index, camera) in self.cameras.iter().enumerate() {
                        ui.selectable_value(&mut self.selected_camera, index, &camera.name);
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Atualizar lista").clicked() {
                    if let Some(commands) = &self.commands {
                        let _ = commands.send(Command::RefreshCameras);
                    }
                }
                if ui
                    .add_enabled(!self.camera_busy, egui::Button::new("Ligar câmera"))
                    .clicked()
                {
                    if let Some(camera) = self.cameras.get(self.selected_camera) {
                        self.camera_device_id = camera.id.clone();
                        let _ = self.save_settings();
                        if let Some(commands) = &self.commands {
                            let _ = commands.send(Command::StartCamera(camera.id.clone()));
                            self.camera_busy = true;
                        }
                    }
                }
            });
        }
        if self.camera_busy {
            ui.spinner();
        }
        if ui
            .checkbox(&mut self.show_stats, "Mostrar estatísticas")
            .changed()
        {
            let _ = self.save_settings();
        }
        if self.show_stats {
            ui.add_space(4.0);
            if let Some(rtt) = self.stats.rtt_ms {
                ui.label(format!("Conexão · RTT {rtt} ms"));
            }
            for stream in &self.stats.outbound {
                ui.label(format!(
                    "Envio · {} · {} FPS · {:.1} Mbps",
                    stream.resolution,
                    stream.fps,
                    stream.bitrate as f64 / 1_000_000.0
                ));
                ui.label(format!(
                    "{} · {} · {}",
                    stream.codec,
                    stream.encoder,
                    if stream.hardware { "GPU" } else { "CPU" }
                ));
            }
            for (index, stream) in self.stats.inbound.iter().enumerate() {
                ui.label(format!(
                    "Recebendo {} · {} · {} FPS · {:.1} Mbps · {} quadros perdidos",
                    index + 1,
                    stream.resolution,
                    stream.fps,
                    stream.bitrate as f64 / 1_000_000.0,
                    stream.dropped
                ));
            }
        }
        ui.add_space(16.0);
        if !self.status.is_empty() {
            ui.label(
                RichText::new(&self.status)
                    .size(13.0)
                    .color(Color32::from_rgb(202, 196, 208)),
            );
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            if ui
                .add_sized(
                    [ui.available_width(), 42.0],
                    egui::Button::new(
                        RichText::new("Sair da sala").color(Color32::from_rgb(255, 180, 171)),
                    )
                    .corner_radius(21),
                )
                .clicked()
            {
                self.cancelled.store(true, Ordering::Relaxed);
                if let Some(commands) = self.commands.take() {
                    let _ = commands.send(Command::Leave);
                }
                self.busy = true;
            }
        });
    }

    fn room_stage(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Transmissões").size(24.0).strong());
            ui.label(
                RichText::new(format!("  {} ativa(s)", self.videos.len()))
                    .size(13.0)
                    .color(Color32::from_rgb(202, 196, 208)),
            );
        });
        ui.add_space(18.0);
        if self.videos.is_empty() {
            let size = ui.available_size();
            egui::Frame::new()
                .fill(Color32::from_rgb(33, 31, 38))
                .corner_radius(24)
                .inner_margin(28)
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(
                        (size.x - 56.0).max(1.0),
                        (size.y - 56.0).max(1.0),
                    ));
                    ui.vertical_centered(|ui| {
                        ui.add_space((size.y * 0.25).max(30.0));
                        ui.label(RichText::new("Tudo pronto por aqui").size(23.0).strong());
                        ui.label(
                            RichText::new(
                                "As telas e câmeras compartilhadas vão aparecer neste espaço.",
                            )
                            .size(14.0)
                            .color(Color32::from_rgb(202, 196, 208)),
                        );
                    });
                });
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (id, video) in &mut self.videos {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(33, 31, 38))
                        .corner_radius(18)
                        .inner_margin(14)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&video.label).size(14.0).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("+").clicked() {
                                            video.zoom = (video.zoom * 1.25).min(4.0);
                                        }
                                        ui.label(format!("{:.0}%", video.zoom * 100.0));
                                        if ui.small_button("−").clicked() {
                                            video.zoom = (video.zoom / 1.25).max(0.5);
                                        }
                                        if ui.small_button("Ajustar").clicked() {
                                            video.zoom = 1.0;
                                        }
                                    },
                                );
                            });
                            let frame = video.frame.lock().ok().and_then(|mut frame| frame.take());
                            if let Some(frame) = frame {
                                if let Some(texture) = &mut video.texture {
                                    texture.set(frame, egui::TextureOptions::LINEAR);
                                } else {
                                    video.texture = Some(ui.ctx().load_texture(
                                        id,
                                        frame,
                                        egui::TextureOptions::LINEAR,
                                    ));
                                }
                            }
                            if let Some(texture) = &video.texture {
                                let size = texture.size_vec2();
                                let fit = (ui.available_width() / size.x).min(1.0);
                                egui::ScrollArea::both().max_height(560.0).show(ui, |ui| {
                                    ui.image((texture.id(), size * fit * video.zoom));
                                });
                            }
                        });
                    ui.add_space(10.0);
                }
            });
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str, password: bool) {
    ui.label(
        RichText::new(label)
            .size(13.0)
            .color(Color32::from_rgb(225, 219, 229)),
    );
    ui.add_sized(
        [ui.available_width(), 42.0],
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .password(password)
            .margin(egui::Margin::symmetric(12, 10)),
    );
}

impl Drop for App {
    fn drop(&mut self) {
        instance::remove_tray();
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(commands) = self.commands.take() {
            let _ = commands.send(Command::Leave);
        }
    }
}
