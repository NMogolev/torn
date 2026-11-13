//! Optional integration test for real containers and codecs.
//!
//! Set `TORN_FFMPEG` to an `ffmpeg` executable (or install `ffmpeg` on `PATH`)
//! to generate the fixtures. Without it the test is a no-op so the normal test
//! suite never depends on an external encoder.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use paleo::{AudioCodec, Event, Media, VideoCodec, VideoExtraDataKind};

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

fn run(ffmpeg: &std::ffi::OsString, args: &[&str], output: &Path) {
    let status = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(output)
        .status()
        .expect("ffmpeg starts");
    assert!(status.success(), "ffmpeg must create {}", output.display());
}

fn audio_only(ffmpeg: &std::ffi::OsString, directory: &Path, name: &str, codec: &str) -> PathBuf {
    let path = directory.join(name);
    run(
        ffmpeg,
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:a",
            codec,
        ],
        &path,
    );
    path
}

fn decode_all(media: &mut Media) -> (Vec<paleo::AudioBuffer>, Vec<paleo::VideoPacket>) {
    let mut audio = Vec::new();
    let mut video = Vec::new();
    loop {
        match media.next_event().expect("decoding succeeds") {
            Event::Audio(buffer) => audio.push(buffer),
            Event::Video(packet) => video.push(packet),
            Event::EndOfStream => break,
        }
        assert!(
            audio.len() + video.len() < 100_000,
            "decoding must terminate"
        );
    }
    (audio, video)
}

#[test]
fn decodes_generated_media() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping: set TORN_FFMPEG or install ffmpeg to run this test");
        return;
    };

    let directory = std::env::temp_dir().join(format!("paleo-ffmpeg-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("the fixture directory is created");

    check_audio_containers(&ffmpeg, &directory);
    check_audio_video_containers(&ffmpeg, &directory);

    let _ = std::fs::remove_dir_all(directory);
}

fn check_audio_containers(ffmpeg: &std::ffi::OsString, directory: &Path) {
    for (name, codec, expected) in [
        ("tone.m4a", "aac", AudioCodec::Aac),
        ("tone.mp3", "libmp3lame", AudioCodec::Mp3),
        ("tone.flac", "flac", AudioCodec::Flac),
        ("tone.ogg", "libvorbis", AudioCodec::Vorbis),
    ] {
        let path = audio_only(ffmpeg, directory, name, codec);
        let mut media = Media::open(&path)
            .unwrap_or_else(|error| panic!("{} must open: {error}", path.display()));
        let audio = media.info().audio().expect("an audio track is selected");
        assert_eq!(audio.codec(), expected, "{name}");
        assert_eq!(audio.sample_rate(), 44_100, "{name}");
        assert_eq!(audio.channels(), 1, "{name}");
        assert!(media.info().video().is_none(), "{name}");

        let (buffers, video) = decode_all(&mut media);
        assert!(video.is_empty(), "{name}");
        let frames: usize = buffers.iter().map(paleo::AudioBuffer::frames).sum();
        assert!(
            frames >= 40_000,
            "{name}: expected about one second, decoded {frames} frames"
        );
        assert!(
            buffers.iter().all(|buffer| buffer.sample_rate() == 44_100),
            "{name}: the decoded rate must stay constant"
        );
        let _ = std::fs::remove_file(path);
    }
}

fn check_audio_video_containers(ffmpeg: &std::ffi::OsString, directory: &Path) {
    // A real A/V container. Symphonia demuxes both tracks; Paleo decodes the
    // audio and hands the H.264 packets through untouched.
    let mp4 = directory.join("clip.mp4");
    run(
        ffmpeg,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=25:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ],
        &mp4,
    );
    let mkv = directory.join("clip.mkv");
    run(ffmpeg, &["-i", mp4.to_str().unwrap(), "-c", "copy"], &mkv);

    for path in [&mp4, &mkv] {
        let mut media = Media::open(path).expect("the clip opens");
        let info = media.info();
        let video = info.video().expect("a video track is selected");
        assert_eq!(video.codec(), VideoCodec::H264);
        assert_eq!((video.width(), video.height()), (320, 240));
        let avc = video
            .avc_decoder_config()
            .expect("the container carries an avcC record");
        assert!(avc.len() > 6, "{}: avcC is not empty", path.display());
        assert!(
            video
                .extra_data()
                .iter()
                .any(|extra| extra.kind() == VideoExtraDataKind::AvcDecoderConfig),
            "{}: the record kind is reported",
            path.display()
        );
        let audio = info.audio().expect("an audio track is selected");
        assert_eq!(audio.codec(), AudioCodec::Aac);
        assert_eq!(audio.channels(), 1);
        let duration = info.duration().expect("the duration is known");
        assert!(
            duration.as_secs_f64() > 0.8 && duration.as_secs_f64() < 1.5,
            "unexpected duration {duration:?}"
        );

        let (buffers, packets) = decode_all(&mut media);
        assert!(!buffers.is_empty(), "{}: audio decodes", path.display());
        assert!(
            packets.len() >= 20,
            "{}: expected about 25 video packets, got {}",
            path.display(),
            packets.len()
        );
        assert!(
            packets.iter().all(|packet| !packet.data().is_empty()),
            "{}: video packets carry data",
            path.display()
        );
        let last = packets
            .iter()
            .map(paleo::VideoPacket::pts)
            .max()
            .unwrap_or(Duration::ZERO);
        assert!(
            last < Duration::from_secs(2),
            "{}: unexpected last video timestamp {last:?}",
            path.display()
        );
    }

    // Seeking an A/V file must reset audio state and keep both tracks usable.
    let mut media = Media::open(&mp4).expect("the clip reopens");
    media
        .seek(Duration::from_millis(500))
        .expect("seeking mid-stream succeeds");
    let (buffers, packets) = decode_all(&mut media);
    assert!(!buffers.is_empty());
    assert!(!packets.is_empty());

    // Reaching end-of-stream must not poison the reader: Symphonia's ISO/MP4
    // atom iterator cannot resume by itself, so Media reopens the source.
    media
        .seek(Duration::from_millis(250))
        .expect("seeking after end-of-stream succeeds");
    let (buffers, packets) = decode_all(&mut media);
    assert!(!buffers.is_empty());
    assert!(!packets.is_empty());
}
