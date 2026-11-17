use livekit::{
    options::TrackPublishOptions,
    prelude::*,
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::{native::NativeAudioSource, AudioSourceOptions, RtcAudioSource},
    },
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct AudioShare {
    pub track: LocalAudioTrack,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for AudioShare {
    fn drop(&mut self) {
        crate::wasapi::stop();
        self.task.abort();
    }
}

pub async fn start(
    room: &Room,
    cancelled: Arc<AtomicBool>,
    failed: impl FnOnce() + Send + 'static,
) -> Result<AudioShare, String> {
    let pid = tokio::task::spawn_blocking(crate::discord::discord_pid)
        .await
        .map_err(|_| "Não foi possível identificar o Discord; áudio desativado.")??;
    if cancelled.load(Ordering::Relaxed) {
        return Err("Conexão encerrada; áudio desativado.".into());
    }
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(5);
    let callback = Arc::new(move |data: Vec<u8>| {
        let _ = tx.try_send(data);
    });
    let started = tokio::task::spawn_blocking(move || crate::wasapi::start(pid, callback))
        .await
        .map_err(|_| "Não foi possível iniciar a captura de áudio.")?;
    if !started {
        return Err("Não foi possível excluir o Discord do áudio; áudio desativado.".into());
    }
    if cancelled.load(Ordering::Relaxed) {
        crate::wasapi::stop();
        return Err("Conexão encerrada; áudio desativado.".into());
    }
    let source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 2, 100);
    let track =
        LocalAudioTrack::create_audio_track("screen-audio", RtcAudioSource::Native(source.clone()));
    let options = TrackPublishOptions {
        source: TrackSource::ScreenshareAudio,
        ..Default::default()
    };
    if room
        .local_participant()
        .publish_track(LocalTrack::Audio(track.clone()), options)
        .await
        .is_err()
    {
        crate::wasapi::stop();
        return Err("Não foi possível publicar o áudio; a tela continua compartilhada.".into());
    }
    if cancelled.load(Ordering::Relaxed) {
        crate::wasapi::stop();
        let _ = room.local_participant().unpublish_track(&track.sid()).await;
        return Err("Conexão encerrada; áudio desativado.".into());
    }
    let task = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            if chunk.len() != 3840 {
                continue;
            }
            let mut frame = AudioFrame::new(48_000, 2, 480);
            for (dst, bytes) in frame.data.to_mut().iter_mut().zip(chunk.chunks_exact(4)) {
                let sample = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                *dst = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            }
            if source.capture_frame(&frame).await.is_err() {
                break;
            }
        }
        failed();
    });
    Ok(AudioShare { track, task })
}
