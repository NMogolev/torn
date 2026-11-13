//! PCM decoding and seeking tests that require no external tools.

// The fixture uses a fixed 0.5 s, 44.1 kHz signal; every cast here stays well
// inside the range of its destination type.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::f32::consts::TAU;
use std::path::PathBuf;
use std::time::Duration;

use paleo::{AudioCodec, Event, Media};

const SAMPLE_RATE: u32 = 44_100;
const SECONDS: f32 = 0.5;

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("paleo-{name}-{}.wav", std::process::id()))
}

fn write_sine_wav(path: &std::path::Path) {
    let frames = (SAMPLE_RATE as f32 * SECONDS) as usize;
    let mut data = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let phase = TAU * 440.0 * frame as f32 / SAMPLE_RATE as f32;
        let sample = (phase.sin() * 0.5 * f32::from(i16::MAX)) as i16;
        data.extend_from_slice(&sample.to_le_bytes());
    }

    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(path, wav).expect("the test wav is written");
}

fn read_all(media: &mut Media) -> Vec<paleo::AudioBuffer> {
    let mut buffers = Vec::new();
    loop {
        match media.next_event().expect("decoding succeeds") {
            Event::Audio(buffer) => buffers.push(buffer),
            Event::Video(_) => panic!("a wav file has no video"),
            Event::EndOfStream => break,
        }
        assert!(buffers.len() < 1024, "decoding must terminate");
    }
    buffers
}

#[test]
fn decodes_pcm_wav() {
    let path = temp_path("decode");
    write_sine_wav(&path);

    let mut media = Media::open(&path).expect("the wav opens");
    let info = media.info().clone();
    assert_eq!(info.format(), "wave");
    let audio = info.audio().expect("an audio track is selected");
    assert_eq!(audio.codec(), AudioCodec::Pcm);
    assert_eq!(audio.sample_rate(), SAMPLE_RATE);
    assert_eq!(audio.channels(), 1);
    assert!(info.video().is_none());

    let expected = (SAMPLE_RATE as f32 * SECONDS) as usize;
    let duration = info.duration().expect("the duration is known");
    assert!(
        duration.abs_diff(Duration::from_secs_f32(SECONDS)) < Duration::from_millis(2),
        "unexpected duration {duration:?}"
    );

    let buffers = read_all(&mut media);
    let frames: usize = buffers.iter().map(paleo::AudioBuffer::frames).sum();
    assert_eq!(frames, expected);

    let mut previous = Duration::ZERO;
    for buffer in &buffers {
        assert_eq!(buffer.sample_rate(), SAMPLE_RATE);
        assert_eq!(buffer.channels(), 1);
        assert!(!buffer.is_empty());
        assert!(buffer.pts() >= previous, "timestamps must not regress");
        previous = buffer.pts();
        for sample in buffer.samples() {
            assert!(sample.is_finite());
            assert!(
                (-1.0..=1.0).contains(sample),
                "sample {sample} is normalized"
            );
        }
    }
    assert!(buffers[0].pts() < Duration::from_millis(5));

    // The end marker is sticky.
    assert!(matches!(
        media.next_event().expect("the end is repeatable"),
        Event::EndOfStream
    ));

    let _ = std::fs::remove_file(path);
}

#[test]
fn seeks_pcm_wav() {
    let path = temp_path("seek");
    write_sine_wav(&path);

    let mut media = Media::open(&path).expect("the wav opens");
    media
        .seek(Duration::from_millis(250))
        .expect("seek succeeds");

    let buffers = read_all(&mut media);
    let first = buffers.first().expect("audio follows the seek");
    assert!(
        first.pts() >= Duration::from_millis(200) && first.pts() <= Duration::from_millis(310),
        "unexpected first timestamp {:?}",
        first.pts()
    );
    let frames: usize = buffers.iter().map(paleo::AudioBuffer::frames).sum();
    let expected = ((SECONDS * 1000.0) as usize - 250) * SAMPLE_RATE as usize / 1000;
    assert!(
        frames.abs_diff(expected) < SAMPLE_RATE as usize / 10,
        "expected about {expected} frames, decoded {frames}"
    );

    // Seeking backwards must reset the decoder and produce audio again.
    media.seek(Duration::ZERO).expect("rewind succeeds");
    let buffers = read_all(&mut media);
    let frames: usize = buffers.iter().map(paleo::AudioBuffer::frames).sum();
    assert_eq!(frames, (SAMPLE_RATE as f32 * SECONDS) as usize);

    let _ = std::fs::remove_file(path);
}
