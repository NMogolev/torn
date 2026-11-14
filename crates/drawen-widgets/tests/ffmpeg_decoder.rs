//! Optional smoke test for the `FFmpeg` command-line backend.
//!
//! Set `DRAWEN_FFMPEG` to an `ffmpeg` executable (or install `ffmpeg` on `PATH`)
//! to run it. Without the variable the test is a no-op so the normal test suite
//! never depends on a system `FFmpeg` installation.

use std::process::Command;
use std::thread;
use std::time::Duration;

use drawen_ui::Widget;
use drawen_widgets::{FfmpegConfig, Video};

fn ffmpeg() -> Option<std::ffi::OsString> {
    std::env::var_os("DRAWEN_FFMPEG").or_else(|| {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|_| std::ffi::OsString::from("ffmpeg"))
    })
}

#[test]
fn decodes_a_generated_clip() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping: set DRAWEN_FFMPEG or install ffmpeg to run this test");
        return;
    };
    let clip = std::env::temp_dir().join(format!("drawen-ffmpeg-smoke-{}.mp4", std::process::id()));
    let generated = Command::new(&ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=10:duration=1",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&clip)
        .status()
        .expect("ffmpeg starts");
    assert!(generated.success(), "ffmpeg must generate the test clip");

    let config = FfmpegConfig::new()
        .with_ffmpeg(ffmpeg)
        .with_frame_rate(10)
        .with_max_dimension(Some(160));
    let mut video = Video::open_with(&clip, config).expect("the clip opens");
    let metadata = video.metadata().expect("metadata is available");
    assert_eq!((metadata.width(), metadata.height()), (160, 90));
    let duration = metadata.duration().as_secs_f32();
    assert!(
        (0.8..1.2).contains(&duration),
        "unexpected duration {duration}"
    );

    for _ in 0..100 {
        video.advance_animation(Duration::from_millis(16), false);
        if video.frame().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let frame = video.frame().expect("a preview frame is decoded");
    assert_eq!((frame.width(), frame.height()), (160, 90));

    video.set_playing(true);
    for _ in 0..100 {
        video.advance_animation(Duration::from_millis(16), false);
        if video.position() >= Duration::from_millis(400) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(video.position() >= Duration::from_millis(400));

    for _ in 0..300 {
        video.advance_animation(Duration::from_millis(16), false);
        if !video.is_playing() && !video.is_animating() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!video.is_playing(), "playback stops at end-of-stream");

    drop(video);
    let _ = std::fs::remove_file(clip);
}
