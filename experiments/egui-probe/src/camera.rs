use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoFrame, VideoRotation},
    video_source::native::NativeVideoSource,
};
use nokhwa::{
    pixel_format::RgbFormat,
    query,
    utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType},
    Camera,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, Sender},
    Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
}

pub fn devices() -> Result<Vec<Device>, String> {
    query(ApiBackend::MediaFoundation)
        .map(|devices| {
            devices
                .into_iter()
                .map(|device| Device {
                    id: device.misc(),
                    name: device.human_name(),
                })
                .collect()
        })
        .map_err(|_| {
            "Não foi possível listar as câmeras. Verifique a permissão de câmera do Windows.".into()
        })
}

pub struct Capture {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    ready: Option<Receiver<Result<(u32, u32), String>>>,
    sink: Option<Sender<NativeVideoSource>>,
}

impl Capture {
    pub fn start(id: String, failed: impl Fn() + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let (sink_tx, sink_rx) = mpsc::channel::<NativeVideoSource>();
        let thread = std::thread::spawn(move || {
            let format =
                RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution);
            let mut camera = match Camera::new(CameraIndex::String(id), format) {
                Ok(camera) => camera,
                Err(_) => {
                    let _ =
                        ready_tx.send(Err("Não foi possível abrir a câmera selecionada.".into()));
                    return;
                }
            };
            if camera.open_stream().is_err() {
                let _ = ready_tx.send(Err("Não foi possível iniciar a câmera selecionada.".into()));
                return;
            }
            let camera_format = camera.camera_format();
            let (width, height) = (
                camera_format.resolution().x(),
                camera_format.resolution().y(),
            );
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                let _ = ready_tx.send(Err("A câmera retornou uma resolução inválida.".into()));
                return;
            }
            if ready_tx.send(Ok((width, height))).is_err() {
                return;
            }
            let sink = loop {
                if thread_stop.load(Ordering::Relaxed) {
                    return;
                }
                match sink_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(sink) => break sink,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => return,
                }
            };
            while !thread_stop.load(Ordering::Relaxed) {
                let frame = match camera
                    .frame()
                    .and_then(|frame| frame.decode_image::<RgbFormat>())
                {
                    Ok(frame) if frame.width() == width && frame.height() == height => frame,
                    _ => {
                        failed();
                        break;
                    }
                };
                let argb = rgb_to_argb(frame.as_raw());
                let mut buffer = I420Buffer::new(width, height);
                let (sy, su, sv) = buffer.strides();
                let (y, u, v) = buffer.data_mut();
                yuv_helper::argb_to_i420(
                    &argb,
                    width * 4,
                    y,
                    sy,
                    u,
                    su,
                    v,
                    sv,
                    width as i32,
                    height as i32,
                );
                if !sink.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, buffer)) {
                    failed();
                    break;
                }
            }
        });
        Self {
            stop,
            thread: Some(thread),
            ready: Some(ready),
            sink: Some(sink_tx),
        }
    }

    pub fn take_ready(&mut self) -> Receiver<Result<(u32, u32), String>> {
        self.ready
            .take()
            .expect("a câmera só pode ser iniciada uma vez")
    }

    pub fn activate(&mut self, sink: NativeVideoSource) -> Result<(), String> {
        self.sink
            .take()
            .ok_or_else(|| "A câmera não está mais ativa.".to_owned())?
            .send(sink)
            .map_err(|_| "A câmera foi interrompida antes de começar.".into())
    }
}

fn rgb_to_argb(rgb: &[u8]) -> Vec<u8> {
    let mut argb = Vec::with_capacity(rgb.len() / 3 * 4);
    for pixel in rgb.chunks_exact(3) {
        argb.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    argb
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.sink.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_rgb_pixels_to_argb_memory_order() {
        assert_eq!(
            rgb_to_argb(&[1, 2, 3, 10, 20, 30]),
            [3, 2, 1, 255, 30, 20, 10, 255]
        );
    }
}
