use eframe::egui::ColorImage;
use futures_util::StreamExt;
use livekit::webrtc::{
    video_frame::{native::VideoFrameBufferExt, VideoFormatType},
    video_stream::native::NativeVideoStream,
};
use std::sync::{Arc, Mutex};

pub type LatestFrame = Arc<Mutex<Option<ColorImage>>>;

pub async fn receive(
    track: livekit::prelude::RemoteVideoTrack,
    latest: LatestFrame,
    ctx: eframe::egui::Context,
) {
    let mut stream = NativeVideoStream::new(track.rtc_track());
    while let Some(frame) = stream.next().await {
        let width = frame.buffer.width();
        let height = frame.buffer.height();
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            continue;
        }
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        macro_rules! convert {
            ($buffer:expr) => {
                $buffer.to_argb(
                    VideoFormatType::ABGR,
                    &mut pixels,
                    width * 4,
                    width as i32,
                    height as i32,
                )
            };
        }
        let buffer = frame.buffer.as_ref();
        if let Some(b) = buffer.as_i420() {
            convert!(b);
        } else if let Some(b) = buffer.as_i420a() {
            convert!(b);
        } else if let Some(b) = buffer.as_i422() {
            convert!(b);
        } else if let Some(b) = buffer.as_i444() {
            convert!(b);
        } else if let Some(b) = buffer.as_i010() {
            convert!(b);
        } else if let Some(b) = buffer.as_nv12() {
            convert!(b);
        } else if let Some(b) = buffer.as_native() {
            convert!(b);
        } else {
            continue;
        }
        if let Ok(mut slot) = latest.lock() {
            *slot = Some(ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                &pixels,
            ));
        }
        ctx.request_repaint();
        tokio::task::yield_now().await;
    }
}
