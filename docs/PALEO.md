# Paleo

Paleo is Torn's media layer. It is not a codec library and not a container
parser: it assembles focused crates behind one streaming API, so Torn can pick
the best implementation for each job instead of depending on a single
monolithic framework.

The first implementation is built on [Symphonia](https://docs.rs/symphonia):

```text
file ──► Symphonia demuxer ──┬──► Symphonia audio decoder ──► AudioBuffer (f32, interleaved)
                             │
                             └──► VideoPacket (compressed) ──► video decoder backend
```

Paleo lives in `crates/paleo`. The `MediaSession` contract and the widgets that
present frames live in `torn-widgets`; see [MEDIA.md](MEDIA.md).

## Why not one framework

The media layer needs four independent things:

1. container demuxing that streams and seeks;
2. audio decoding for many codecs;
3. video decoding for H.264, HEVC, AV1, and VP8/VP9;
4. encoding when Torn needs to export media.

No single pure-Rust crate is currently best at all four. Symphonia is the de
facto standard for demuxing and audio decoding: it is widely used, permissively
licensed (`MPL-2.0`), `#![forbid(unsafe_code)]`, and reads MP4, Matroska/WebM,
Ogg, WAV, AIFF, CAF, FLAC, and MPEG audio without loading the file into memory.
Its video support is experimental and has no production decoders, so Paleo takes
video packets from the demuxer and leaves decoding to dedicated crates.

This was also why Rumpeg was rejected as the foundation. It is a complete
FFmpeg-shaped framework, but its MP4 demuxer reads the whole file, drops AAC
tracks written by FFmpeg, and seeks to non-keyframe samples; its scale layer
cannot convert YUV 4:2:0 to RGBA directly, and it has no audio output. Fixing
those defects would mean owning and maintaining a fork of a one-month-old
project. Paleo instead composes crates that already solve those problems.

## Memory model

Paleo deliberately keeps no internal queues and never reads an entire file:

| Allocation | Bound |
| --- | --- |
| Source buffer | fixed 64 KiB (`MediaSourceStream`) |
| Container indexes | owned by the demuxer, bounded by the container |
| Decoder state | one audio decoder, one frame at a time |
| Returned data | one `AudioBuffer` or `VideoPacket` per `next_event` |
| Video frames | none: packets are handed to the caller |

`Media::next_event` returns owned values, so a frame or packet is freed as soon
as the session has consumed it. The video path has no decode-ahead buffer by
design; a `MediaSession` implementation decides how many frames to keep for
scheduling.

Measured with the `scan` example (release build, Windows, x86-64). Peak working
set is sampled while the process runs:

| Input | Size | Work | Peak working set | Throughput |
| --- | --- | --- | --- | --- |
| PCM WAV, 48 kHz stereo, 600 s | 110 MB | decode to `f32` | 4.8 MB | 6201x realtime |
| AAC in MP4, 48 kHz stereo, 120 s | 1.9 MB | decode to `f32` | 4.4 MB | 2024x realtime |
| H.264 720p30 + AAC in MP4, 30 s | 10.7 MB | demux video, decode audio | 4.4 MB | 848x realtime |

Throughput covers demuxing and audio decoding only; the 720p row moves 900
H.264 packets (10.4 MB) without buffering them.

Reproduce with:

```text
cargo run -p paleo --release --example scan -- big.wav
```

## Supported media

Containers: ISO/MP4, Matroska/WebM, Ogg, Wave, AIFF, CAF, FLAC, and MPEG audio.

Audio codecs: AAC-LC, ALAC, FLAC, MP1/MP2/MP3, Vorbis, PCM, and ADPCM.

Not yet decodable: HE-AAC and HE-AACv2 (Symphonia marks them unfinished) and
Opus (recognized by the demuxer, no decoder in Symphonia). A file whose
selected audio track has no decoder fails to open with
`Error::UnsupportedCodec`; the file can still be inspected by a future
video-only session that tolerates missing audio.

Video: Paleo demuxes H.264, HEVC, AV1, VP8, VP9, MPEG-4 Part 2, and Motion JPEG
packets. Decoding is not implemented yet.

## API

```rust
use std::time::Duration;
use paleo::{Event, Media};

let mut media = Media::open("clip.mp4")?;
if let Some(video) = media.info().video() {
    println!("{} {}x{}", video.codec(), video.width(), video.height());
}

loop {
    match media.next_event()? {
        Event::Audio(audio) => { /* feed the audio device */ }
        Event::Video(packet) => { /* feed a video decoder */ }
        Event::EndOfStream => break,
    }
}

media.seek(Duration::from_secs(10))?;
```

`AudioBuffer` contains interleaved `f32` samples normalized to `-1.0..=1.0`,
with the presentation timestamp, sample rate, and channel count of the block.
`VideoPacket` contains the compressed payload and both timestamps; it carries no
keyframe flag because Symphonia 0.6 does not expose packet flags, so a video
decoder must recognize its own random-access points from the bitstream.

Only the default audio and video tracks are selected, matching the container's
default-track flag and falling back to the first track with a known codec.

## Video backends

The packet API exists so video decoding can be added without changing the
container layer:

| Codec | Candidate | Notes |
| --- | --- | --- |
| H.264 | `rusty_h264-decoder` | BSD-2, safe core, validated against openh264; needs an `avcC` → decoder-parameter bridge |
| AV1 | `rav1d` | Rust port of dav1d; low-level API, needs an integration layer |
| VP8/VP9 | `libvpx` bindings | C dependency; would need a per-package `unsafe_code` exception like `torn-servo-*` |
| HEVC | none mature | Symphonia parses `hvcC`; decoding remains open |

Backends are feature-gated crates that convert `VideoPacket` plus
`VideoTrackInfo` into RGBA frames. The first target is H.264, because it is the
common denominator for MP4/Matroska test media and the existing FFmpeg session
already establishes the expected frame timestamps.

Paleo does not convert YUV to RGBA itself. A backend owns that conversion and
chooses its pixel format; this avoids a mandatory full-frame copy for every
frame.

## Testing

- `cargo test -p paleo` runs PCM tests that build a WAV fixture in memory and
  verify decode counts, timestamps, normalization, and seek behavior.
- Setting `TORN_FFMPEG` additionally runs `tests/ffmpeg_formats.rs`, which
  generates AAC, MP3, FLAC, Vorbis, MP4/H.264, and Matroska fixtures with a real
  FFmpeg and checks track selection, decoding, packet timestamps, and A/V seek.

The optional test follows the existing `torn-widgets` FFmpeg smoke test: no
system FFmpeg is required for the normal suite.

## Limits and next steps

Not implemented yet:

- video decoding and YUV → RGBA conversion;
- audio output and resampling (an output device belongs to the platform layer,
  not to Paleo; see [MEDIA.md](MEDIA.md));
- HE-AAC, Opus, and HEVC decoding;
- multiple audio or video tracks, subtitles, chapters, and attachments;
- network sources and adaptive buffering;
- encoding.

Roadmap:

1. H.264 decoder backend over `rusty_h264-decoder`, with RGBA output and
   keyframe-aware seeking.
2. `PaleoSession` in `torn-widgets` implementing `MediaSession`, keeping
   `FfmpegSession` as the fallback.
3. Audio output device plus an audio-mastered `MediaClock`.
4. AV1 (decode) and AV1/VP9 (encode) backends as separate feature-gated crates.
