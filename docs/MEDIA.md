# Media

Media support in Torn is split into three layers: a timing and scheduling
contract, a decode/playback session behind that contract, and widgets that only
present frames and forward user intent. This document defines the contract and
the two current backends: the out-of-process `FFmpeg` command-line backend and
the in-process Paleo/H.264 backend. It is the reference to read before adding
audio, streaming, or a new decoder.

## Layers

```text
MediaClock          presentation time, rate, state
    │
MediaSession        decode, transport, buffering, failure
    │
Video               view and controls
```

A view never owns the decoder. `Video` holds an optional `Box<dyn MediaSession>`
plus a `MediaClock` that is used when the application supplies frames manually.
All position, duration, rate, and playing-state queries resolve through the
session when one is attached.

## Clock

`MediaClock` is the single playback timeline. It stores a position, an optional
duration, a rate, and a `MediaState`:

| State | Meaning |
| --- | --- |
| `Idle` | Nothing has been opened. |
| `Buffering` | The session is filling its buffer. |
| `Playing` | Position advances. |
| `Paused` | Position is held. |
| `Ended` | Position reached the duration. |
| `Failed` | A fatal session error stopped playback. |

The clock is a value type. The view or session advances it in one of two ways:

- **Decoder-only sessions** call `MediaClock::advance(delta)` (or
  `advance_looping`) with the time since the previous update.
- **Sessions with audio output** call `MediaClock::synchronize(position)` with
  the audio device position. Audio is the master clock because resampling and
  device latency make the output position authoritative; the wall clock is not.

`advance` applies `rate`, clamps at the duration, and moves to `Ended` unless the
clock is looping. `advance_looping` preserves the overshoot modulo the duration.
`seek` clamps to the known duration and leaves an idle, ended, or failed clock
paused at the new position. `set_rate` clamps to `0.05..=16.0` and rejects
non-finite values. `finish` and `fail` record terminal states without inventing
a position.

Rates above `1.0` change presentation speed only. A backend that cannot decode
faster than real time must either drop frames or report `Buffering`; it must not
advance the clock ahead of the frames it can deliver.

## Frame scheduling

`MediaFrame` couples an RGBA `RasterImage` with a presentation timestamp and the
duration for which the frame stays visible. `MediaFrameScheduler` owns a small
bounded queue (four frames) and decides what to paint:

- `push` rejects frames that already expired or arrive out of order.
- `present(position)` discards expired frames and selects the newest frame whose
  timestamp has been reached. Frames that are early are never shown early,
  except as an initial preview when nothing has been presented yet.
- `seek(position)` drops queued frames but keeps the visible image, so a seek
  does not flash an empty surface while the decoder restarts.
- `replace(frame)` installs an application-supplied frame immediately.
- `current()` is what a view paints.

The scheduler is deliberately not a decoder buffer. It holds no encoded data,
performs no I/O, and does not decide when to block. Backends own their own
buffering and deliver decoded frames through `MediaUpdate`.

## Sessions

`MediaSession` is the transport and decode contract:

```rust
trait MediaSession {
    fn metadata(&self) -> Option<&MediaMetadata>;
    fn clock(&self) -> &MediaClock;
    fn clock_mut(&mut self) -> &mut MediaClock;
    fn play(&mut self) -> Result<(), MediaError>;
    fn pause(&mut self);
    fn seek(&mut self, position: Duration) -> Result<(), MediaError>;
    fn set_rate(&mut self, rate: f64);
    fn update(&mut self, delta: Duration) -> MediaUpdate;
    fn is_active(&self) -> bool;
}
```

`update` is called once per animation tick. It advances or synchronizes the
clock, returns at most the newest frame, and may report an error or a state
change. A session must be cheap when nothing changed: a paused session returns an
empty update instead of waking a decoder.

`MediaMetadata` reports the display width and height after rotation, plus the
container duration (zero when unknown). `MediaError` is a displayable fatal
error. Recoverable conditions belong in `MediaState::Buffering`, not in an error.

A session may restart its decoder internally on seek. That is an implementation
detail; the contract only requires that the clock and subsequent frames agree on
the requested position.

## Widget rules

- `Video` paints `MediaFrameScheduler::current()` and forwards play, pause, seek,
  rate, and loop requests to the session.
- `Video::set_frame` is the manual path for applications that already own a
  decoder. It installs the frame through the scheduler; the widget advances its
  own `MediaClock` so progress and time labels stay live.
- `AnimatedImage` and `Image` are not sessions. A GIF is a small, fully decoded
  timeline, not a media pipeline, and it keeps its own frame index.
- Audio playback must subscribe to the same clock rather than becoming an
  independent widget with its own timer. Adding a separate `AudioPlayer` before
  the clock was shared is how video and audio drift apart.

## FFmpeg command-line backend

The first backend is intentionally out of process:

- `ffprobe` reads width, height, rotation, and duration.
- `ffmpeg` decodes to raw RGBA at a requested frame rate and maximum dimension.
- A worker thread reads fixed-size frames and posts the newest one; a slow UI
  thread drops frames instead of growing an unbounded backlog.
- Seeking kills the process and restarts it at the requested position.
- The decoded size is capped at 1280 pixels by default; rotation swaps the
  reported width and height before scaling.
- `TORN_FFMPEG` and `TORN_FFPROBE` select the executables. `FfmpegConfig` can
  point at a portable build shipped next to the application.

Frame timestamps assigned by the reader are `start + index / frame_rate`. That is
accurate for constant-frame-rate output and is why the backend is suitable for
local clips, previews, onboarding media, and media cards, but not for exact frame
stepping.

Known limits:

- raw frames cross the CPU boundary and are copied into `RasterImage`, the
  display list, and then a GPU texture;
- no audio track, so there is no audio/video clock relationship yet;
- no hardware decode or zero-copy external textures;
- no buffering policy, network source, adaptive bitrate, or subtitle track;
- seek precision depends on `ffmpeg` and keyframe placement.

These are backend limits, not contract limits. A new backend implements
`MediaSession`; widget code does not change.

## Paleo backend

`PaleoSession` (in `torn-widgets`) is the in-process backend. It opens a file
with `paleo::Media`, demuxes video packets with Symphonia, and decodes H.264
with `paleo_h264::H264Decoder`:

- `Video::open_paleo(path)` is the convenience entry point; `Video::with_session`
  attaches any other `MediaSession` implementation.
- Files open video-only, so an audio codec that this build cannot decode does
  not prevent playback and no audio is decoded before an output device exists.
- Decoding is synchronous and bounded on the calling thread. Each `update`
  decodes at most a few packets and keeps a couple of pictures ahead of the
  playhead instead of building an unbounded queue. There is no worker thread
  yet, so a large frame can briefly block the UI thread.
- Seeking resets the decoder and decodes a preview even while paused, so the
  view can repaint immediately.
- `PaleoSession` stores the source path and reopens it when seeking after
  end-of-stream. Symphonia 0.6's ISO/MP4 reader cannot resume once its atom
  iterator has reached EOF; `paleo::Media` does the same for direct callers.
- B-pictures are ordered by the decoder's `PicOrderCnt`, because Symphonia 0.6
  does not implement the ISO/MP4 `ctts` composition offsets. See
  [PALEO.md](PALEO.md) for the framing and timestamp details.

Known limits:

- H.264 only (4:2:0 frame pictures); AV1, VP8/VP9, and HEVC still need a
  backend;
- no audio output, resampling, or audio-mastered clock yet;
- no hardware decode or zero-copy textures.

## Adding a backend

1. Probe or negotiate metadata before playback and build a `MediaClock` with the
   duration.
2. Decode on a worker or platform callback, timestamp frames with presentation
   time, and deliver only the newest frame through `MediaUpdate`.
3. Restart or flush on `seek`, and keep the clock position equal to the first
   frame the backend will deliver.
4. If the backend has audio, synchronize the clock from the output position and
   let video frames follow the clock rather than the wall clock.
5. Report fatal failures through `MediaError` and the clock's `Failed` state.

In-process and hardware backends are expected to keep the same contract. Their
value is removing copies and improving seek accuracy, not changing the view API.

## Future work

The next milestones, in order:

1. An audio output device and an audio-mastered clock shared by audio and video.
2. Explicit buffering, pause-on-underflow, and end-of-stream reporting for
   network sources.
3. Subtitle and metadata tracks.
4. Additional Paleo video backends: AV1, VP8/VP9, and HEVC once a mature
   pure-Rust decoder exists.
5. In-process decoding, hardware decode, and GPU external textures without
   changing `Video` or `MediaSession` callers.

Until then, documentation and one coherent example product are more valuable
than additional media surface area. That example should exercise media with a
clear fallback and error state rather than demonstrate every widget in the
toolkit.
