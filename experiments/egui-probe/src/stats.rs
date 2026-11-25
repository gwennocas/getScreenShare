use livekit::{rtc_engine::SessionStats, webrtc::stats::RtcStats};
use std::{collections::HashMap, time::Instant};

#[derive(Clone, Debug, Default)]
pub struct Stream {
    pub resolution: String,
    pub fps: u32,
    pub bitrate: u64,
    pub codec: String,
    pub encoder: String,
    pub hardware: bool,
    pub dropped: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub outbound: Vec<Stream>,
    pub inbound: Vec<Stream>,
    pub rtt_ms: Option<u32>,
}

pub fn collect(stats: &SessionStats, previous: &mut HashMap<String, (u64, Instant)>) -> Snapshot {
    let now = Instant::now();
    let codecs = stats
        .publisher_stats
        .iter()
        .chain(&stats.subscriber_stats)
        .filter_map(|entry| match entry {
            RtcStats::Codec(codec) => Some((codec.rtc.id.clone(), codec.codec.mime_type.clone())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut snapshot = Snapshot::default();
    for entry in &stats.publisher_stats {
        match entry {
            RtcStats::OutboundRtp(video) if video.stream.kind == "video" => {
                snapshot.outbound.push(Stream {
                    resolution: resolution(video.outbound.frame_width, video.outbound.frame_height),
                    fps: video.outbound.frames_per_second.round() as u32,
                    bitrate: bitrate(
                        &format!("out:{}", video.rtc.id),
                        video.sent.bytes_sent,
                        now,
                        previous,
                    ),
                    codec: codecs
                        .get(&video.stream.codec_id)
                        .cloned()
                        .unwrap_or_default(),
                    encoder: video.outbound.encoder_implementation.clone(),
                    hardware: video.outbound.power_efficient_encoder,
                    dropped: 0,
                });
            }
            RtcStats::CandidatePair(pair) if pair.candidate_pair.nominated => {
                let ms = pair.candidate_pair.current_round_trip_time * 1000.0;
                if ms > 0.0 && ms.is_finite() {
                    snapshot.rtt_ms = Some(ms.round() as u32);
                }
            }
            _ => {}
        }
    }
    for entry in &stats.subscriber_stats {
        if let RtcStats::InboundRtp(video) = entry {
            if video.stream.kind != "video" {
                continue;
            }
            snapshot.inbound.push(Stream {
                resolution: resolution(video.inbound.frame_width, video.inbound.frame_height),
                fps: video.inbound.frames_per_second.round() as u32,
                bitrate: bitrate(
                    &format!("in:{}", video.rtc.id),
                    video.inbound.bytes_received,
                    now,
                    previous,
                ),
                codec: codecs
                    .get(&video.stream.codec_id)
                    .cloned()
                    .unwrap_or_default(),
                encoder: video.inbound.decoder_implementation.clone(),
                hardware: video.inbound.power_efficient_decoder,
                dropped: video.inbound.frames_dropped,
            });
        }
    }
    snapshot
}

fn resolution(width: u32, height: u32) -> String {
    if width == 0 || height == 0 {
        "—".into()
    } else {
        format!("{width}×{height}")
    }
}

fn bitrate(
    key: &str,
    bytes: u64,
    now: Instant,
    previous: &mut HashMap<String, (u64, Instant)>,
) -> u64 {
    let result = previous.get(key).map_or(0, |(last_bytes, last_time)| {
        let seconds = now.duration_since(*last_time).as_secs_f64();
        if seconds <= 0.0 {
            0
        } else {
            (bytes.saturating_sub(*last_bytes) as f64 * 8.0 / seconds) as u64
        }
    });
    previous.insert(key.to_owned(), (bytes, now));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_uses_byte_delta_and_handles_counter_reset() {
        let now = Instant::now();
        let mut previous = HashMap::from([(
            "out:1".into(),
            (1_000, now - std::time::Duration::from_secs(2)),
        )]);
        assert_eq!(bitrate("out:1", 2_000, now, &mut previous), 4_000);
        assert_eq!(
            bitrate(
                "out:1",
                500,
                now + std::time::Duration::from_secs(1),
                &mut previous
            ),
            0
        );
    }
}
