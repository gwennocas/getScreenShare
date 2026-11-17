use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, SampleFormat, SizedSample,
};
use futures_util::StreamExt;
use livekit::{prelude::RemoteAudioTrack, webrtc::audio_stream::native::NativeAudioStream};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

type Buffer = Arc<Mutex<VecDeque<i16>>>;

fn write_output<T: SizedSample + FromSample<i16>>(
    output: &mut [T],
    channels: usize,
    queue: &Buffer,
) {
    let mut queue = queue.try_lock().ok();
    for frame in output.chunks_mut(channels) {
        let left = queue.as_mut().and_then(|q| q.pop_front()).unwrap_or(0);
        let right = queue.as_mut().and_then(|q| q.pop_front()).unwrap_or(0);
        for (index, sample) in frame.iter_mut().enumerate() {
            let value = if channels == 1 {
                ((left as i32 + right as i32) / 2) as i16
            } else if index == 0 {
                left
            } else if index == 1 {
                right
            } else {
                0
            };
            *sample = T::from_sample(value);
        }
    }
}

fn push_samples(queue: &mut VecDeque<i16>, samples: &[i16], capacity: usize) {
    let capacity = capacity & !1;
    if capacity == 0 {
        queue.clear();
        return;
    }
    let samples = &samples[..samples.len() & !1];
    let skip = samples.len().saturating_sub(capacity) & !1;
    let samples = &samples[skip..];
    while queue.len() + samples.len() > capacity {
        queue.pop_front();
    }
    queue.extend(samples);
}

pub struct Audio {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    #[cfg(test)]
    frames: Arc<AtomicU64>,
}

impl Audio {
    pub fn start(track: RemoteAudioTrack, failed: impl Fn(String) + Send + 'static) -> Self {
        let (stop, stop_rx) = tokio::sync::oneshot::channel();
        let frames = Arc::new(AtomicU64::new(0));
        let count = frames.clone();
        let thread = std::thread::spawn(move || {
            if let Err(error) = run(track, stop_rx, count) {
                failed(error);
            }
        });
        Self {
            stop: Some(stop),
            thread: Some(thread),
            #[cfg(test)]
            frames,
        }
    }
    #[cfg(test)]
    pub fn frame_count(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }
}

fn run(
    track: RemoteAudioTrack,
    stop: tokio::sync::oneshot::Receiver<()>,
    frames: Arc<AtomicU64>,
) -> Result<(), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("Nenhum dispositivo de saída de áudio disponível.")?;
    let supported = device
        .default_output_config()
        .map_err(|_| "Não foi possível configurar a saída de áudio.")?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    if channels == 0 {
        return Err("Dispositivo de áudio inválido.".into());
    }
    let rate = config.sample_rate.0;
    let queue: Buffer = Arc::new(Mutex::new(VecDeque::new()));
    let output_queue = queue.clone();
    let error = |err| eprintln!("Erro na saída de áudio: {err}");
    let stream = match format {
        SampleFormat::F32 => device.build_output_stream(
            &config,
            move |out: &mut [f32], _| write_output(out, channels, &output_queue),
            error,
            Some(Duration::from_secs(3)),
        ),
        SampleFormat::I16 => device.build_output_stream(
            &config,
            move |out: &mut [i16], _| write_output(out, channels, &output_queue),
            error,
            Some(Duration::from_secs(3)),
        ),
        SampleFormat::U16 => device.build_output_stream(
            &config,
            move |out: &mut [u16], _| write_output(out, channels, &output_queue),
            error,
            Some(Duration::from_secs(3)),
        ),
        _ => return Err("Formato de áudio de saída não suportado.".into()),
    }
    .map_err(|_| "Não foi possível abrir a saída de áudio.")?;
    stream
        .play()
        .map_err(|_| "Não foi possível iniciar a saída de áudio.")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Não foi possível iniciar o áudio.")?;
    runtime.block_on(async move {
        let mut incoming = NativeAudioStream::new(track.rtc_track(), rate as i32, 2);
        tokio::pin!(stop);
        loop {
            tokio::select! {
                _ = &mut stop => break,
                frame = incoming.next() => match frame {
                    Some(frame) => if let Ok(mut buffer) = queue.lock() { push_samples(&mut buffer, &frame.data, rate as usize * 2 / 5); frames.fetch_add(1, Ordering::Relaxed); },
                    None => break,
                }
            }
        }
    });
    drop(stream);
    Ok(())
}

impl Drop for Audio {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_buffer_is_bounded_and_preserves_recent_stereo_samples() {
        let mut queue = VecDeque::from(vec![1, 2, 3, 4]);
        push_samples(&mut queue, &[5, 6, 7, 8], 6);
        assert_eq!(queue, VecDeque::from(vec![3, 4, 5, 6, 7, 8]));
        push_samples(&mut queue, &[9, 10, 11, 12, 13, 14, 15, 16], 6);
        assert_eq!(queue, VecDeque::from(vec![11, 12, 13, 14, 15, 16]));
        push_samples(&mut queue, &[1, 2, 3, 4, 5, 6], 5);
        assert_eq!(queue, VecDeque::from(vec![3, 4, 5, 6]));
    }
}
