use livekit::webrtc::{
    desktop_capturer::{DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions},
    native::yuv_helper,
    video_frame::{I420Buffer, VideoFrame, VideoRotation},
    video_source::native::NativeVideoSource,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender},
    Arc,
};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Source {
    pub id: u64,
    pub title: String,
    pub kind: DesktopCaptureSourceType,
}

pub fn sources() -> Vec<Source> {
    let mut result = Vec::new();
    for kind in [
        DesktopCaptureSourceType::Screen,
        DesktopCaptureSourceType::Window,
    ] {
        if let Some(capturer) = DesktopCapturer::new(DesktopCapturerOptions::new(kind)) {
            result.extend(capturer.get_source_list().into_iter().map(|source| {
                let id = source.id();
                Source {
                    id,
                    title: match (kind, source.title()) {
                        (DesktopCaptureSourceType::Screen, title) if title.is_empty() => {
                            format!("Tela {}", id.saturating_add(1))
                        }
                        (DesktopCaptureSourceType::Window, title) if title.is_empty() => {
                            "Janela sem título".into()
                        }
                        (_, title) => title,
                    },
                    kind,
                }
            }));
        }
    }
    result
}

pub fn dimensions(source: Source) -> Result<(u32, u32), String> {
    let mut capturer = DesktopCapturer::new(DesktopCapturerOptions::new(source.kind))
        .ok_or("Não foi possível iniciar a captura.")?;
    let selected = capturer
        .get_source_list()
        .into_iter()
        .find(|item| item.id() == source.id)
        .ok_or("A tela ou janela selecionada não está mais disponível.")?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    capturer.start_capture(Some(selected), move |frame| match frame {
        Ok(frame) => {
            let _ = tx.try_send(Some((frame.width(), frame.height())));
        }
        Err(livekit::webrtc::desktop_capturer::CaptureError::Permanent) => {
            let _ = tx.try_send(None);
        }
        Err(livekit::webrtc::desktop_capturer::CaptureError::Temporary) => {}
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let (width, height) = loop {
        if Instant::now() >= deadline {
            return Err("A captura demorou para responder.".into());
        }
        capturer.capture_frame();
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(Some(dimensions)) => break dimensions,
            Ok(None) => return Err("Não foi possível capturar a fonte selecionada.".into()),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err("A captura foi interrompida.".into()),
        }
    };
    if width <= 0 || height <= 0 || width > 8192 || height > 8192 {
        return Err("A fonte retornou dimensões inválidas.".into());
    }
    Ok((width as u32, height as u32))
}

#[cfg(windows)]
pub fn refresh_rate(source: &Source) -> Result<u32, String> {
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::HWND,
            Graphics::Gdi::{
                EnumDisplayDevicesW, EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow,
                DEVMODEW, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, MONITORINFO, MONITORINFOEXW,
                MONITOR_DEFAULTTONEAREST,
            },
        },
    };
    let mut name = [0u16; 32];
    unsafe {
        match source.kind {
            DesktopCaptureSourceType::Screen => {
                // Os identificadores de tela do capturador WebRTC correspondem ao índice de EnumDisplayDevicesW.
                let index = u32::try_from(source.id)
                    .map_err(|_| "Não foi possível identificar o monitor.")?;
                let mut device = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                if !EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0).as_bool() {
                    return Err("Não foi possível identificar o monitor.".into());
                }
                name.copy_from_slice(&device.DeviceName);
            }
            DesktopCaptureSourceType::Window => {
                let monitor =
                    MonitorFromWindow(HWND(source.id as usize as *mut _), MONITOR_DEFAULTTONEAREST);
                let mut info = MONITORINFOEXW::default();
                info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
                if !GetMonitorInfoW(monitor, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
                    return Err("Não foi possível identificar o monitor da janela.".into());
                }
                name.copy_from_slice(&info.szDevice);
            }
        }
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        if !EnumDisplaySettingsW(PCWSTR(name.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode).as_bool()
            || !(2..=1000).contains(&mode.dmDisplayFrequency)
        {
            return Err(
                "Não foi possível descobrir a taxa do monitor. Escolha 30 ou 60 FPS.".into(),
            );
        }
        Ok(mode.dmDisplayFrequency)
    }
}

pub fn screen_bitrate(width: u32, height: u32, fps: u32) -> u64 {
    let base = (u64::from(width) * u64::from(height) * 4_000_000) / (1920 * 1080);
    (base * u64::from(fps.clamp(30, 1000) + 30) / 60).clamp(2_000_000, 30_000_000)
}

pub struct Capture {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    ready: Option<Receiver<Result<(), String>>>,
    #[cfg(test)]
    frames: Arc<AtomicU64>,
}

impl Capture {
    pub fn start(
        source: Source,
        sink: NativeVideoSource,
        fps: u32,
        cancelled: Arc<AtomicBool>,
        failed: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let frames = Arc::new(AtomicU64::new(0));
        let captured_frames = frames.clone();
        let failed = Arc::new(failed);
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let mut options = DesktopCapturerOptions::new(source.kind);
            options.set_include_cursor(true);
            let Some(mut capturer) = DesktopCapturer::new(options) else {
                let _ = ready_tx.send(Err("Não foi possível iniciar a captura.".into()));
                failed();
                return;
            };
            let Some(selected) = capturer
                .get_source_list()
                .into_iter()
                .find(|s| s.id() == source.id)
            else {
                let _ = ready_tx.send(Err(
                    "A tela ou janela selecionada não está mais disponível.".into(),
                ));
                failed();
                return;
            };
            let capture_stop = thread_stop.clone();
            let mut ready_tx = Some(ready_tx);
            let callback_failed = failed.clone();
            capturer.start_capture(Some(selected), move |result| {
                let frame = match result {
                    Ok(frame) => frame,
                    Err(livekit::webrtc::desktop_capturer::CaptureError::Temporary) => return,
                    Err(livekit::webrtc::desktop_capturer::CaptureError::Permanent) => {
                        fail_capture(
                            &mut ready_tx,
                            &capture_stop,
                            callback_failed.as_ref(),
                            "A captura foi interrompida pelo sistema.",
                        );
                        return;
                    }
                };
                let (width, height, stride) = (frame.width(), frame.height(), frame.stride());
                if width <= 0
                    || height <= 0
                    || stride < (width as u32).saturating_mul(4)
                    || frame.data().len() < (stride as usize).saturating_mul(height as usize)
                {
                    fail_capture(
                        &mut ready_tx,
                        &capture_stop,
                        callback_failed.as_ref(),
                        "A captura retornou um quadro inválido.",
                    );
                    return;
                }
                let mut buffer = I420Buffer::new(width as u32, height as u32);
                let (sy, su, sv) = buffer.strides();
                let (y, u, v) = buffer.data_mut();
                yuv_helper::argb_to_i420(frame.data(), stride, y, sy, u, su, v, sv, width, height);
                if sink.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, buffer)) {
                    captured_frames.fetch_add(1, Ordering::Relaxed);
                    if let Some(ready_tx) = ready_tx.take() {
                        let _ = ready_tx.send(Ok(()));
                    }
                } else {
                    fail_capture(
                        &mut ready_tx,
                        &capture_stop,
                        callback_failed.as_ref(),
                        "Não foi possível enviar o quadro ao LiveKit.",
                    );
                }
            });
            let interval = Duration::from_secs_f64(1.0 / f64::from(fps.clamp(1, 1000)));
            let mut next = Instant::now();
            while !thread_stop.load(Ordering::Relaxed) && !cancelled.load(Ordering::Relaxed) {
                capturer.capture_frame();
                next += interval;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else {
                    next = now;
                }
            }
        });
        Self {
            stop,
            thread: Some(thread),
            ready: Some(ready),
            #[cfg(test)]
            frames,
        }
    }
    pub fn take_ready(&mut self) -> Receiver<Result<(), String>> {
        self.ready
            .take()
            .expect("a captura só pode ser iniciada uma vez")
    }
    #[cfg(test)]
    pub fn frame_count(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }
}

fn fail_capture(
    ready: &mut Option<SyncSender<Result<(), String>>>,
    stop: &AtomicBool,
    failed: &(dyn Fn() + Send + Sync),
    message: &str,
) {
    if let Some(ready) = ready.take() {
        let _ = ready.send(Err(message.to_owned()));
    }
    if !stop.swap(true, Ordering::Relaxed) {
        failed();
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screen_bitrate_scales_with_resolution_and_has_limits() {
        assert_eq!(screen_bitrate(1280, 720, 30), 2_000_000);
        assert_eq!(screen_bitrate(1920, 1080, 30), 4_000_000);
        assert_eq!(screen_bitrate(1920, 1080, 60), 6_000_000);
        assert_eq!(screen_bitrate(3840, 2160, 30), 16_000_000);
        assert_eq!(screen_bitrate(8192, 8192, 30), 30_000_000);
    }

    #[test]
    #[ignore = "requer uma sessão interativa Windows com monitor"]
    fn captures_a_screen_frame() {
        let mut capturer = DesktopCapturer::new(DesktopCapturerOptions::new(
            DesktopCaptureSourceType::Screen,
        ))
        .expect("capturador de tela");
        let source = capturer
            .get_source_list()
            .into_iter()
            .next()
            .expect("monitor disponível");
        let resolved = Source {
            id: source.id(),
            title: source.title(),
            kind: DesktopCaptureSourceType::Screen,
        };
        assert!(refresh_rate(&resolved).expect("taxa do monitor") >= 2);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        capturer.start_capture(Some(source), move |frame| {
            let result = frame.map(|frame| {
                (
                    frame.width(),
                    frame.height(),
                    frame.stride(),
                    frame.data().len(),
                )
            });
            let _ = sender.try_send(result);
        });
        capturer.capture_frame();
        let (width, height, stride, size) = receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("resposta da captura")
            .expect("quadro de tela");
        assert!(width > 0 && height > 0);
        assert!(stride >= width as u32 * 4);
        assert!(size >= stride as usize * height as usize);
    }
}
