//! Prints track metadata and decode totals for a media file.
//!
//! ```text
//! cargo run -p paleo --release --example scan -- clip.mp4
//! ```

use std::time::{Duration, Instant};

use paleo::{Event, Media};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: scan <file>");
        return;
    };
    if let Err(error) = scan(&path) {
        eprintln!("error: {error}");
    }
}

fn scan(path: &str) -> Result<(), paleo::Error> {
    let started = Instant::now();
    let mut media = Media::open(path)?;
    let info = media.info().clone();
    println!("format: {}", info.format());
    println!(
        "duration: {}",
        info.duration()
            .map_or_else(|| "unknown".to_owned(), fmt_time)
    );
    if let Some(audio) = info.audio() {
        println!(
            "audio: {} {} Hz {} ch",
            audio.codec(),
            audio.sample_rate(),
            audio.channels()
        );
    }
    if let Some(video) = info.video() {
        println!(
            "video: {} {}x{}{}",
            video.codec(),
            video.width(),
            video.height(),
            video
                .frame_rate()
                .map_or_else(String::new, |rate| format!(" @ {rate:.3} fps"))
        );
    }

    let mut audio_frames = 0_u64;
    let mut video_packets = 0_u64;
    let mut video_bytes = 0_u64;
    let mut first_pts = None;
    let mut last_pts = Duration::ZERO;
    loop {
        match media.next_event()? {
            Event::Audio(audio) => audio_frames += audio.frames() as u64,
            Event::Video(packet) => {
                video_packets += 1;
                video_bytes += packet.data().len() as u64;
                first_pts.get_or_insert(packet.pts());
                last_pts = last_pts.max(packet.pts());
            }
            Event::EndOfStream => break,
        }
    }

    let elapsed = started.elapsed();
    println!("audio frames: {audio_frames}");
    println!(
        "video packets: {video_packets} ({video_bytes} bytes, first {:?}, last {:?})",
        first_pts.unwrap_or_default(),
        last_pts
    );
    let media_time = info.duration().unwrap_or(last_pts);
    let speed = if elapsed.is_zero() {
        0.0
    } else {
        media_time.as_secs_f64() / elapsed.as_secs_f64()
    };
    println!("decoded in {elapsed:.3?} ({speed:.1}x realtime)");
    Ok(())
}

fn fmt_time(time: Duration) -> String {
    format!("{:.3} s", time.as_secs_f64())
}
