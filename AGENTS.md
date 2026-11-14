# Drawen agent notes

Before generating Drawen application code, read
[`docs/AI_GUIDE.md`](docs/AI_GUIDE.md) and copy one of the canonical examples.

Non-negotiables:

- Application code uses the `drawen` facade; add `drawen-platform-winit` for a
  native window. Do not import `drawen-ui`, `drawen-widgets`, `drawen-core`, or
  `drawen-render` in ordinary app code.
- The widget tree, `Signal`, `Subscription`, and `UiRuntime` are UI-thread-only.
  Never move them to a worker thread.
- Widgets do not spawn threads or timers and do not call platform APIs. Use
  typed signals, `EventContext` redraw/layout requests,
  `Widget::advance_animation`, and `UiRuntime::next_frame_in`.
- `Button::new()` takes no label. Append a `Text` child.
- State is plain Rust. On a state change, invalidate paint and return
  `WindowAction::RequestRedraw`; invalidate layout only when geometry changes.
- DSS is a typed subset: compound type/id/class/state selectors only. No
  descendant selectors, animations, or full browser CSS.
- Media backends are explicit: `Video::open_paleo` (H.264, no audio),
  `Video::open_paleo_audio` (feature `audio`), `Video::open` (external FFmpeg,
  no audio). Background playback after the last window closes requires
  `MediaDirector::with_background_playback(true)`.

Canonical files:

- `crates/drawen/examples/minimal_window.rs`
- `crates/drawen/examples/stateful_app.rs`
- `docs/RECIPES.md`
- `docs/MEDIA.md`
