use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenRequest<'a> {
    room: &'a str,
    identity: String,
    name: &'a str,
    access_code: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
    url: String,
}

fn endpoint(raw: &str) -> Result<reqwest::Url, String> {
    let mut url =
        reqwest::Url::parse(raw.trim()).map_err(|_| "URL do servidor inválida.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("A URL do servidor deve começar com http:// ou https://.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("A URL do servidor não pode conter credenciais.".into());
    }
    let path = format!("{}/token", url.path().trim_end_matches('/'));
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub async fn connect(
    server: &str,
    code: &str,
    room: &str,
    name: &str,
) -> Result<
    (
        livekit::Room,
        tokio::sync::mpsc::UnboundedReceiver<livekit::RoomEvent>,
    ),
    String,
> {
    let room = room.trim();
    let name = name.trim();
    if room.is_empty()
        || room.chars().count() > 64
        || !room
            .chars()
            .all(|c| c.is_alphanumeric() || "_- ".contains(c))
    {
        return Err("Nome de sala inválido. Use letras, números, espaço, - e _.".into());
    }
    if name.is_empty() || name.encode_utf16().count() > 40 {
        return Err("Informe um nome com até 40 caracteres.".into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| "Não foi possível iniciar a conexão.".to_owned())?;
    let response = client
        .post(endpoint(server)?)
        .json(&TokenRequest {
            room,
            identity: uuid::Uuid::new_v4().to_string(),
            name,
            access_code: code,
        })
        .send()
        .await
        .map_err(|_| "Não foi possível falar com o servidor de tokens.".to_owned())?;
    match response.status().as_u16() {
        401 => return Err("Código de acesso incorreto.".into()),
        429 => return Err("Muitas tentativas. Espere um minuto e tente novamente.".into()),
        status if !(200..300).contains(&status) => {
            return Err(format!("O servidor respondeu com erro {status}."))
        }
        _ => {}
    }
    let data: TokenResponse = response
        .json()
        .await
        .map_err(|_| "Resposta inválida do servidor de tokens.".to_owned())?;
    if data.token.is_empty() || data.url.is_empty() {
        return Err("Resposta inválida do servidor de tokens.".into());
    }
    let options = livekit::RoomOptions::default();
    livekit::Room::connect(&data.url, &data.token, options)
        .await
        .map_err(|_| "Não foi possível entrar na sala LiveKit.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_preserve_subpaths_and_reject_credentials() {
        assert_eq!(
            endpoint("https://example.com/base/?x=1").unwrap().as_str(),
            "https://example.com/base/token"
        );
        assert!(endpoint("https://user:secret@example.com").is_err());
        assert!(endpoint("file:///tmp/server").is_err());
    }
}

#[cfg(test)]
mod server_tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[tokio::test]
    async fn rejected_access_code_is_reported() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let bytes = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..bytes]).starts_with("POST /token HTTP/1.1"));
            stream
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let error =
            match super::connect(&format!("http://{address}"), "wrong", "sala", "pessoa").await {
                Ok(_) => panic!("o servidor recusou o código"),
                Err(error) => error,
            };
        server.join().unwrap();
        assert_eq!(error, "Código de acesso incorreto.");
    }
}

#[cfg(test)]
mod live_tests {
    #[tokio::test]
    #[ignore = "requer servidores locais de teste"]
    async fn connects_to_livekit_room() {
        let server = std::env::var("GSS_TEST_TOKEN_URL").expect("GSS_TEST_TOKEN_URL");
        let (room, _events) = super::connect(&server, "test", "gss-test", "Teste")
            .await
            .expect("conexão à sala");
        assert_eq!(room.name(), "gss-test");
        room.close().await.expect("encerramento da sala");
    }
}

#[cfg(test)]
mod media_tests {
    use futures_util::StreamExt;
    use livekit::webrtc::{
        video_frame::{I420Buffer, VideoFrame, VideoRotation},
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
        video_stream::native::NativeVideoStream,
    };
    use livekit::{
        options::{TrackPublishOptions, VideoCodec},
        prelude::*,
    };
    use std::time::Duration;

    #[tokio::test]
    #[ignore = "requer servidores locais de teste"]
    async fn sends_video_between_two_participants() {
        let server = std::env::var("GSS_TEST_TOKEN_URL").expect("GSS_TEST_TOKEN_URL");
        let (sender, _) = super::connect(&server, "test", "gss-video", "Emissor")
            .await
            .unwrap();
        let (receiver, mut events) = super::connect(&server, "test", "gss-video", "Receptor")
            .await
            .unwrap();
        let source = NativeVideoSource::new(
            VideoResolution {
                width: 64,
                height: 64,
            },
            true,
        );
        let track =
            LocalVideoTrack::create_video_track("test", RtcVideoSource::Native(source.clone()));
        let options = TrackPublishOptions {
            source: TrackSource::Screenshare,
            video_codec: VideoCodec::VP8,
            simulcast: false,
            ..Default::default()
        };
        sender
            .local_participant()
            .publish_track(LocalTrack::Video(track), options)
            .await
            .unwrap();
        let remote = tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(event) = events.recv().await {
                if let RoomEvent::TrackSubscribed {
                    track: RemoteTrack::Video(track),
                    ..
                } = event
                {
                    return track;
                }
            }
            panic!("sala encerrada antes da assinatura");
        })
        .await
        .expect("tempo para receber a faixa");
        let mut stream = NativeVideoStream::new(remote.rtc_track());
        let mut got_frame = false;
        for _ in 0..30 {
            source.capture_frame(&VideoFrame::new(
                VideoRotation::VideoRotation0,
                I420Buffer::new_black(64, 64),
            ));
            if let Ok(Some(frame)) =
                tokio::time::timeout(Duration::from_millis(300), stream.next()).await
            {
                assert_eq!((frame.buffer.width(), frame.buffer.height()), (64, 64));
                got_frame = true;
                break;
            }
        }
        assert!(got_frame, "nenhum quadro recebido");
        sender.close().await.unwrap();
        receiver.close().await.unwrap();
    }
}

#[cfg(test)]
mod real_capture_tests {
    use livekit::{
        options::{TrackPublishOptions, VideoCodec},
        prelude::*,
        webrtc::{
            desktop_capturer::DesktopCaptureSourceType,
            video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
        },
    };
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        time::Duration,
    };

    #[tokio::test]
    #[ignore = "requer LiveKit local e sessão interativa Windows com monitor"]
    async fn publishes_a_captured_screen() {
        let server = std::env::var("GSS_TEST_TOKEN_URL").expect("GSS_TEST_TOKEN_URL");
        let (sender, _) = super::connect(&server, "test", "gss-screen", "Emissor")
            .await
            .unwrap();
        let (receiver, mut events) = super::connect(&server, "test", "gss-screen", "Receptor")
            .await
            .unwrap();
        let source = crate::media::sources()
            .into_iter()
            .find(|source| source.kind == DesktopCaptureSourceType::Screen)
            .expect("monitor disponível");
        let (width, height) = crate::media::dimensions(source.clone()).expect("dimensões da fonte");
        let sink = NativeVideoSource::new(VideoResolution { width, height }, true);
        let track =
            LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(sink.clone()));
        sender
            .local_participant()
            .publish_track(
                LocalTrack::Video(track),
                TrackPublishOptions {
                    source: TrackSource::Screenshare,
                    video_codec: VideoCodec::H264,
                    simulcast: false,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let remote = tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(event) = events.recv().await {
                if let RoomEvent::TrackSubscribed {
                    track: RemoteTrack::Video(track),
                    ..
                } = event
                {
                    return track;
                }
            }
            panic!("sala encerrada antes da assinatura");
        })
        .await
        .expect("tempo para receber faixa");
        let failed = Arc::new(AtomicBool::new(false));
        let failed_capture = failed.clone();
        let capture = crate::media::Capture::start(
            source,
            sink,
            30,
            Arc::new(AtomicBool::new(false)),
            move || {
                failed_capture.store(true, Ordering::Relaxed);
            },
        );
        for _ in 0..50 {
            if capture.frame_count() > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            capture.frame_count() > 0,
            "nenhum quadro capturado enviado ao WebRTC"
        );
        let latest = Arc::new(Mutex::new(None));
        let task = tokio::spawn(crate::video::receive(
            remote,
            latest.clone(),
            eframe::egui::Context::default(),
        ));
        let image = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Some(image) = latest.lock().unwrap().take() {
                    break image;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("tempo para receber e converter vídeo");
        assert!(image.size[0] > 0 && image.size[1] > 0);
        task.abort();
        assert!(!failed.load(Ordering::Relaxed), "captura falhou");
        drop(capture);
        sender.close().await.unwrap();
        receiver.close().await.unwrap();
    }
}

#[cfg(test)]
mod audio_tests {
    use livekit::{
        options::TrackPublishOptions,
        prelude::*,
        webrtc::{
            audio_frame::AudioFrame,
            audio_source::{native::NativeAudioSource, AudioSourceOptions, RtcAudioSource},
        },
    };
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    #[tokio::test]
    #[ignore = "requer LiveKit local e dispositivo de áudio de saída"]
    async fn plays_received_silence_without_opening_microphone() {
        let server = std::env::var("GSS_TEST_TOKEN_URL").expect("GSS_TEST_TOKEN_URL");
        let (sender, _) = super::connect(&server, "test", "gss-audio", "Emissor")
            .await
            .unwrap();
        let (receiver, mut events) = super::connect(&server, "test", "gss-audio", "Receptor")
            .await
            .unwrap();
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48000, 2, 200);
        let track =
            LocalAudioTrack::create_audio_track("audio", RtcAudioSource::Native(source.clone()));
        sender
            .local_participant()
            .publish_track(LocalTrack::Audio(track), TrackPublishOptions::default())
            .await
            .unwrap();
        let remote = tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(event) = events.recv().await {
                if let RoomEvent::TrackSubscribed {
                    track: RemoteTrack::Audio(track),
                    ..
                } = event
                {
                    return track;
                }
            }
            panic!("sala encerrou antes da assinatura");
        })
        .await
        .expect("tempo para receber áudio");
        let errors = Arc::new(Mutex::new(None));
        let error_slot = errors.clone();
        let audio = crate::audio::Audio::start(remote, move |error| {
            *error_slot.lock().unwrap() = Some(error);
        });
        for _ in 0..100 {
            source
                .capture_frame(&AudioFrame::new(48000, 2, 480))
                .await
                .unwrap();
            if audio.frame_count() > 0 || errors.lock().unwrap().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let error = errors.lock().unwrap().clone();
        assert!(error.is_none(), "saída de áudio indisponível: {error:?}");
        assert!(audio.frame_count() > 0, "nenhum quadro de áudio recebido");
        drop(audio);
        sender.close().await.unwrap();
        receiver.close().await.unwrap();
    }
}

#[cfg(test)]
mod wasapi_tests {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use futures_util::StreamExt;
    use livekit::{prelude::*, webrtc::audio_stream::native::NativeAudioStream};
    use std::{
        sync::{atomic::AtomicBool, Arc},
        time::Duration,
    };

    #[tokio::test]
    #[ignore = "requer LiveKit local, Discord aberto e dispositivo de áudio"]
    async fn publishes_filtered_loopback_audio() {
        let device = cpal::default_host()
            .default_output_device()
            .expect("saída de áudio");
        let supported = device
            .default_output_config()
            .expect("configuração de áudio");
        let format = supported.sample_format();
        let config = supported.into();
        let err = |e| eprintln!("saída de teste: {e}");
        let silence = match format {
            cpal::SampleFormat::F32 => {
                device.build_output_stream(&config, |data: &mut [f32], _| data.fill(0.0), err, None)
            }
            cpal::SampleFormat::I16 => {
                device.build_output_stream(&config, |data: &mut [i16], _| data.fill(0), err, None)
            }
            cpal::SampleFormat::U16 => device.build_output_stream(
                &config,
                |data: &mut [u16], _| data.fill(32768),
                err,
                None,
            ),
            _ => panic!("formato de saída não suportado"),
        }
        .expect("saída silenciosa");
        silence.play().expect("reprodução silenciosa");
        let server = std::env::var("GSS_TEST_TOKEN_URL").expect("GSS_TEST_TOKEN_URL");
        let (sender, _) = super::connect(&server, "test", "gss-loopback", "Emissor")
            .await
            .unwrap();
        let (receiver, mut events) = super::connect(&server, "test", "gss-loopback", "Receptor")
            .await
            .unwrap();
        let audio = crate::send_audio::start(&sender, Arc::new(AtomicBool::new(false)), || {})
            .await
            .expect("captura filtrada");
        let remote = tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(event) = events.recv().await {
                if let RoomEvent::TrackSubscribed {
                    track: RemoteTrack::Audio(track),
                    ..
                } = event
                {
                    return track;
                }
            }
            panic!("sala encerrou antes da assinatura");
        })
        .await
        .expect("tempo para receber faixa");
        let mut stream = NativeAudioStream::new(remote.rtc_track(), 48000, 2);
        let frame = tokio::time::timeout(Duration::from_secs(15), stream.next())
            .await
            .expect("tempo para receber áudio")
            .expect("áudio recebido");
        assert!(!frame.data.is_empty());
        drop(audio);
        drop(silence);
        sender.close().await.unwrap();
        receiver.close().await.unwrap();
    }
}
