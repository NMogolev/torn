//! Optional end-to-end test for the pure-Rust Paleo video session.
//!
//! Set `DRAWEN_FFMPEG` to an `ffmpeg` executable (or install `ffmpeg` on `PATH`)
//! to run it. Without the variable the test is a no-op, so the normal suite
//! never depends on an external encoder.

use std::process::Command;
use std::time::Duration;

use drawen_ui::Widget;
use drawen_widgets::Video;

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

fn generate(ffmpeg: &std::ffi::OsString, clip: &std::path::Path) {
    let status = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x90:rate=25:duration=1",
            "-c:v",
            "libx264",
            "-profile:v",
            "high",
            "-bf",
            "2",
            "-x264-params",
            "keyint=10:min-keyint=10:scenecut=0",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(clip)
        .status()
        .expect("ffmpeg starts");
    assert!(status.success(), "ffmpeg must generate {}", clip.display());
}

#[test]
fn plays_and_seeks_a_generated_clip() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping: set DRAWEN_FFMPEG or install ffmpeg to run this test");
        return;
    };
    let clip = std::env::temp_dir().join(format!("drawen-paleo-{}.mp4", std::process::id()));
    generate(&ffmpeg, &clip);

    let mut video = Video::open_paleo(&clip).expect("the clip opens");
    assert_eq!(video.decoder_error(), None);
    let metadata = video.metadata().expect("metadata is available");
    assert_eq!((metadata.width(), metadata.height()), (160, 90));
    let duration = metadata.duration().as_secs_f32();
    assert!(
        (0.8..1.2).contains(&duration),
        "unexpected duration {duration}"
    );

    // Opening decodes a preview, so the first picture is ready while paused.
    assert!(video.advance_animation(Duration::ZERO, false));
    let preview = video.frame().expect("a preview frame is decoded");
    assert_eq!((preview.width(), preview.height()), (160, 90));

    video.set_playing(true);
    assert!(video.is_playing());
    for _ in 0..600 {
        video.advance_animation(Duration::from_millis(10), false);
        if video.is_ended() {
            break;
        }
    }
    assert!(video.is_ended(), "playback reaches end-of-stream");
    assert!(!video.is_playing());
    assert_eq!(video.decoder_error(), None);

    // Seeking while paused must decode a replacement preview.
    let target = Duration::from_millis(500);
    video.set_position(target);
    assert_eq!(video.position(), target);
    assert!(video.advance_animation(Duration::ZERO, false));
    assert!(video.frame().is_some(), "a seek preview is decoded");
    assert_eq!(video.decoder_error(), None);

    drop(video);
    let _ = std::fs::remove_file(clip);
}
