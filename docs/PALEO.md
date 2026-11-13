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
                                                                  │
                                          paleo-h264 ──► YUV 4:2:0 ──► RGBA frame
```

Paleo lives in `crates/paleo`. The H.264 backend lives in `crates/paleo-h264`.
The `MediaSession` contract and the widgets that present frames live in
`torn-widgets`; see [MEDIA.md](MEDIA.md).

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
| Video demux | none: packets are handed to the caller |
| `paleo-h264` reorder | at most `reorder_depth` YUV pictures (default 4) |

`Media::next_event` returns owned values, so a frame or packet is freed as soon
as the session has consumed it. The demuxer has no decode-ahead buffer by
design; a `MediaSession` implementation decides how many frames to keep for
scheduling. `paleo-h264` keeps only the decoded pictures that cannot yet be
emitted in display order, and recycles each YUV frame into the next decode.

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
packets. H.264 is decodable through `paleo-h264`; the other codecs still need a
backend. Track metadata exposes the container's codec configuration records
(`VideoTrackInfo::extra_data`), including the H.264 `avcC` record that carries
SPS/PPS and the NAL length size.

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
Symphonia 0.6's ISO/MP4 reader does not implement `ctts`, so it reports
`pts == dts` in decode order. `paleo-h264` therefore orders B-pictures by their
decoded `PicOrderCnt` and assigns presentation timestamps from the packet
timestamps rather than trusting `pts` directly.

Only the default audio and video tracks are selected, matching the container's
default-track flag and falling back to the first track with a known codec.

## Video backends

The packet API exists so video decoding can be added without changing the
container layer:

| Codec | Backend | Status |
| --- | --- | --- |
| H.264 | `paleo-h264` over `rusty_h264-decoder` | implemented: `avcC` bridge, POC reorder, RGBA output |
| AV1 | `rav1d` | candidate: Rust port of dav1d, low-level API |
| VP8/VP9 | `libvpx` bindings | candidate: C dependency, needs a per-package `unsafe_code` exception like `torn-servo-*` |
| HEVC | none mature | Symphonia parses `hvcC`; decoding remains open |

Backends are separate crates that convert `VideoPacket` plus `VideoTrackInfo`
into RGBA frames. Paleo does not convert YUV itself: a backend owns the
conversion and chooses its pixel format, which avoids a mandatory full-frame
copy for every frame.

`paleo-h264` is the first backend. It:

- parses the `avcC` record for SPS/PPS and the NAL length size, and converts
  length-prefixed packets to Annex-B access units, prepending parameter sets at
  every IDR;
- decodes with one streaming `rusty_h264_decoder::Decoder`, with no packet
  queue and no whole-GOP buffering;
- reorders B-pictures by `PicOrderCnt` with a bounded window
  (`DEFAULT_REORDER_DEPTH`, 4 pictures; adjustable with `set_reorder_depth`);
- converts YUV 4:2:0 to tightly packed RGBA with fixed-point limited-range
  BT.601 for SD and BT.709 for 720 lines and above (`ColorMatrix::Auto`);
- drops inter pictures after a seek until the next IDR, so resuming is
  repredictable rather than corrupt.

`rusty_h264-decoder` is pinned exactly (`=0.17.2`) because the packet bridge is
written against its streaming API.

## Testing

- `cargo test -p paleo` runs PCM tests that build a WAV fixture in memory and
  verify decode counts, timestamps, normalization, and seek behavior.
- `cargo test -p paleo-h264` runs unit tests for the `avcC` parser, NAL framing,
  and RGBA conversion.
- Setting `TORN_FFMPEG` additionally runs `paleo/tests/ffmpeg_formats.rs` and
  `paleo-h264/tests/decode.rs`. They generate AAC, MP3, FLAC, Vorbis,
  MP4/H.264, and Matroska fixtures with a real FFmpeg, then check track
  selection, audio decoding, packet timestamps, A/V seek, and H.264
  frame-for-frame agreement with FFmpeg's decode, including a strict B-pyramid.
  The H.264 comparison allows a mean per-channel difference below 1 after
  matching FFmpeg's nearest-neighbour chroma upsampling.

The optional tests follow the existing `torn-widgets` FFmpeg smoke test: no
system FFmpeg is required for the normal suite.

## Limits and next steps

Not implemented yet:

- AV1, VP8/VP9, and HEVC decoding;
- audio output and resampling (an output device belongs to the platform layer,
  not to Paleo; see [MEDIA.md](MEDIA.md));
- HE-AAC and Opus decoding;
- multiple audio or video tracks, subtitles, chapters, and attachments;
- network sources and adaptive buffering;
- encoding.

Known limits of the H.264 backend: `rusty_h264-decoder` decodes 4:2:0 frame
pictures only (no field/MBAFF or 4:2:2/4:4:4), and the reorder window uses a
fixed depth rather than reading `max_num_reorder_frames` from the SPS VUI.
Streams with a deeper reorder pattern can raise the depth explicitly.

Roadmap:

1. `PaleoSession` in `torn-widgets` implementing `MediaSession`, keeping
   `FfmpegSession` as the fallback.
2. Audio output device plus an audio-mastered `MediaClock`.
3. AV1 (decode) and VP8/VP9 (decode/encode) backends as separate crates.
4. Opus and HE-AAC decoder backends; HEVC once a mature pure-Rust decoder
   exists.
