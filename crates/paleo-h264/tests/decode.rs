//! Optional end-to-end test against real H.264 media.
//!
//! Set `TORN_FFMPEG` to an `ffmpeg` executable (or install `ffmpeg` on
//! `PATH`) to run it. Without it the test is a no-op, so the normal suite
//! never depends on an external encoder.

#![allow(clippy::cast_precision_loss)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use paleo::{Event, Media, VideoPacket};
use paleo_h264::{H264Decoder, VideoFrame};

const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const FRAMES: usize = 50;

fn ffmpeg() -> Option<std::ffi::OsString> {
    std::env::var_os("TORN_FFMPEG").or_else(|| {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|_| std::ffi::OsString::from("ffmpeg"))
    })
}

fn generate(ffmpeg: &std::ffi::OsString, output: &Path) {
    generate_with(ffmpeg, output, "320x240", &["-bf", "2"]);
}

fn generate_with(ffmpeg: &std::ffi::OsString, output: &Path, size: &str, extra: &[&str]) {
    let filter = format!("testsrc2=size={size}:rate=25:duration=2");
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(&filter)
        .args(["-c:v", "libx264", "-profile:v", "high"]);
    command.args(extra);
    command.args([
        "-x264-params",
        "keyint=25:min-keyint=25:scenecut=0",
        "-pix_fmt",
        "yuv420p",
    ]);
    let status = command.arg(output).status().expect("ffmpeg starts");
    assert!(status.success(), "ffmpeg must create {}", output.display());
}

fn packets(path: &Path) -> (Vec<VideoPacket>, Vec<paleo::VideoExtraData>) {
    let mut media = Media::open(path).expect("the clip opens");
    let video = media.info().video().expect("a video track is selected");
    let extra_data = video.extra_data().to_vec();
    let mut packets = Vec::new();
    loop {
        match media.next_event().expect("demuxing succeeds") {
            Event::Video(packet) => packets.push(packet),
            Event::Audio(_) => {}
            Event::EndOfStream => break,
        }
    }
    (packets, extra_data)
}

fn decode(packets: &[VideoPacket], extra_data: &[paleo::VideoExtraData]) -> Vec<VideoFrame> {
    let mut decoder = H264Decoder::new();
    decoder
        .configure(extra_data)
        .expect("the avcC record is valid");
    let mut frames = Vec::new();
    for packet in packets {
        frames.extend(decoder.decode(packet).expect("every packet decodes"));
    }
    frames.extend(decoder.flush());
    frames
}

fn frame_bytes(ffmpeg: &std::ffi::OsString, path: &Path) -> Vec<u8> {
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        // Match the backend's nearest-neighbour chroma upsampling, so the
        // comparison isolates bitstream decoding from resampling filters.
        .args([
            "-sws_flags",
            "neighbor",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ])
        .output()
        .expect("ffmpeg converts the clip");
    assert!(output.status.success(), "ffmpeg must convert the clip");
    output.stdout
}

fn mean_absolute_difference(actual: &[u8], expected: &[u8]) -> f64 {
    assert_eq!(actual.len(), expected.len(), "frame sizes must match");
    let total: u64 = actual
        .iter()
        .zip(expected)
        .map(|(actual, expected)| u64::from(actual.abs_diff(*expected)))
        .sum();
    total as f64 / actual.len() as f64
}

#[test]
fn decodes_generated_clip() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping: set TORN_FFMPEG or install ffmpeg to run this test");
        return;
    };
    let clip = std::env::temp_dir().join(format!("paleo-h264-{}.mp4", std::process::id()));
    generate(&ffmpeg, &clip);

    let (packets, extra_data) = packets(&clip);
    assert!(packets.len() >= FRAMES, "got {} packets", packets.len());

    let frames = decode(&packets, &extra_data);
    assert_eq!(frames.len(), FRAMES, "every picture is emitted");

    let mut previous = Duration::ZERO;
    for frame in &frames {
        assert_eq!((frame.width(), frame.height()), (320, 240));
        assert_eq!(
            frame.pixels().len(),
            WIDTH * HEIGHT * 4,
            "RGBA is tightly packed"
        );
        assert_eq!(frame.stride(), WIDTH * 4);
        assert!(
            frame.pts() >= previous,
            "frames are emitted in presentation order"
        );
        previous = frame.pts();
    }
    assert_eq!(frames[0].pts(), Duration::ZERO);

    // The decoder is documented as pixel-exact against ffmpeg. Allow a small
    // rounding margin for the fixed-point RGB matrix and chroma upsampling.
    assert_matches_ffmpeg(&ffmpeg, &clip, &frames, WIDTH, HEIGHT);
    let _ = std::fs::remove_file(clip);
}

/// Small frames have NAL units of 256-511 bytes, whose four-byte length prefix
/// (`00 00 01 xx`) begins with the same bytes as a three-byte Annex-B start
/// code. A caller that trusts the prefix shape fails on exactly these streams.
#[test]
fn decodes_small_frames_with_start_code_like_lengths() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping: set TORN_FFMPEG or install ffmpeg to run this test");
        return;
    };
    let clip = std::env::temp_dir().join(format!("paleo-h264-small-{}.mp4", std::process::id()));
    generate_with(&ffmpeg, &clip, "160x90", &["-bf", "2"]);

    let (packets, extra_data) = packets(&clip);
    let frames = decode(&packets, &extra_data);
    assert_eq!(frames.len(), FRAMES, "every picture is emitted");
    for frame in &frames {
        assert_eq!((frame.width(), frame.height()), (160, 90));
    }
    assert_matches_ffmpeg(&ffmpeg, &clip, &frames, 160, 90);

    let _ = std::fs::remove_file(clip);
}

/// A strict B-pyramid reorders pictures three deep, which exercises the whole
/// default reorder window.
#[test]
fn decodes_hierarchical_b_frames() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping: set TORN_FFMPEG or install ffmpeg to run this test");
        return;
    };
    let clip = std::env::temp_dir().join(format!("paleo-h264-pyramid-{}.mp4", std::process::id()));
    generate_with(
        &ffmpeg,
        &clip,
        "320x240",
        &["-bf", "3", "-x264-params", "b-pyramid=strict"],
    );

    let (packets, extra_data) = packets(&clip);
    let frames = decode(&packets, &extra_data);
    assert_eq!(frames.len(), FRAMES, "every picture is emitted");
    assert!(
        frames
            .windows(2)
            .all(|window| window[0].pts() < window[1].pts()),
        "frames are emitted in presentation order"
    );
    assert_matches_ffmpeg(&ffmpeg, &clip, &frames, WIDTH, HEIGHT);

    let _ = std::fs::remove_file(clip);
}

fn assert_matches_ffmpeg(
    ffmpeg: &std::ffi::OsString,
    clip: &Path,
    frames: &[VideoFrame],
    width: usize,
    height: usize,
) {
    let reference = frame_bytes(ffmpeg, clip);
    let frame_bytes = width * height * 4;
    assert_eq!(reference.len(), frames.len() * frame_bytes);
    for (index, frame) in frames.iter().enumerate() {
        let start = index * frame_bytes;
        let expected = &reference[start..start + frame_bytes];
        let difference = mean_absolute_difference(frame.pixels(), expected);
        assert!(
            difference < 1.0,
            "frame {index} (pts {:?}) differs from ffmpeg by {difference:.3} per channel",
            frame.pts()
        );
    }
}

#[test]
fn resumes_at_a_keyframe_after_reset() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping: set TORN_FFMPEG or install ffmpeg to run this test");
        return;
    };
    let clip = std::env::temp_dir().join(format!("paleo-h264-seek-{}.mp4", std::process::id()));
    generate(&ffmpeg, &clip);

    let (packets, extra_data) = packets(&clip);
    let mut decoder = H264Decoder::new();
    decoder
        .configure(&extra_data)
        .expect("the avcC record is valid");

    let keyframe = packets
        .iter()
        .position(|packet| {
            decoder.is_keyframe(packet) && packet.pts() >= Duration::from_millis(900)
        })
        .expect("the clip has a keyframe near one second");
    assert_eq!(packets[keyframe].pts(), Duration::from_secs(1));

    // A reset makes inter pictures unusable until the next IDR.
    decoder.reset();
    assert!(!decoder.is_keyframe(&packets[keyframe + 1]));
    assert!(
        decoder
            .decode(&packets[keyframe + 1])
            .expect("skipping is not an error")
            .is_empty()
    );

    let mut frames = Vec::new();
    for packet in &packets[keyframe..] {
        frames.extend(decoder.decode(packet).expect("the rest decodes"));
    }
    frames.extend(decoder.flush());

    assert!(
        frames.len() >= 24,
        "expected the tail of the clip, got {} frames",
        frames.len()
    );
    assert_eq!(frames[0].pts(), Duration::from_secs(1));
    assert!(
        frames
            .windows(2)
            .all(|window| window[0].pts() <= window[1].pts())
    );

    let _ = std::fs::remove_file(clip);
}
