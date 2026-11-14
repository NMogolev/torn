//! Optional end-to-end test for audio-mastered Paleo playback.
//!
//! This test needs both an encoder and a real output device, so it is
//! double-gated:
//!
//! - set `TORN_FFMPEG` to an `ffmpeg` executable (or install it on `PATH`);
//! - set `TORN_AUDIO_DEVICE_TEST` to opt into opening the default output.
//!
//! Without both variables the test is a no-op, so the normal suite never
//! depends on an external encoder or on audio hardware.

#![cfg(feature = "audio")]

use std::process::Command;
use std::time::Duration;

use torn_ui::Widget;
use torn_widgets::Video;

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
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
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
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-shortest",
        ])
        .arg(clip)
        .status()
        .expect("ffmpeg starts");
    assert!(status.success(), "ffmpeg must generate {}", clip.display());
}

#[test]
fn follows_the_audio_clock_and_seeks() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping: set TORN_FFMPEG or install ffmpeg to run this test");
        return;
    };
    if std::env::var_os("TORN_AUDIO_DEVICE_TEST").is_none() {
        eprintln!("skipping: set TORN_AUDIO_DEVICE_TEST to open the default output device");
        return;
    }
    let clip = std::env::temp_dir().join(format!("torn-paleo-audio-{}.mp4", std::process::id()));
    generate(&ffmpeg, &clip);

    let mut video = Video::open_paleo_audio(&clip).expect("the clip opens with audio");
    assert_eq!(video.decoder_error(), None);
    let metadata = video.metadata().expect("metadata is available");
    assert_eq!((metadata.width(), metadata.height()), (160, 90));
    let duration = metadata.duration().as_secs_f32();
    assert!(
        (0.8..1.2).contains(&duration),
        "unexpected duration {duration}"
    );

    // Opening decodes ahead of the playhead; the first update presents it.
    assert!(video.advance_animation(Duration::ZERO, false));
    let preview = video.frame().expect("a preview frame is decoded");
    assert_eq!((preview.width(), preview.height()), (160, 90));

    // The clock follows the device, not the delta passed to the widget, so
    // position only advances while audio is actually rendering.
    video.set_playing(true);
    assert!(video.is_playing());
    let mut advanced = Duration::ZERO;
    for _ in 0..120 {
        video.advance_animation(Duration::from_millis(10), false);
        advanced = video.position();
        if advanced >= Duration::from_millis(300) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        advanced >= Duration::from_millis(200),
        "the audio clock should advance playback, got {advanced:?}"
    );
    assert_eq!(video.decoder_error(), None);
    assert!(video.frame().is_some(), "video follows the audio clock");

    video.set_playing(false);
    assert!(!video.is_playing());
    let held = video.position();
    std::thread::sleep(Duration::from_millis(80));
    video.advance_animation(Duration::from_millis(80), false);
    assert!(
        video.position() <= held + Duration::from_millis(60),
        "pausing must stop the clock"
    );

    let target = Duration::from_millis(500);
    video.set_position(target);
    assert_eq!(video.position(), target);
    assert!(video.advance_animation(Duration::ZERO, false));
    assert!(video.frame().is_some(), "a seek preview is decoded");
    assert_eq!(video.decoder_error(), None);

    drop(video);
    let _ = std::fs::remove_file(clip);
}
