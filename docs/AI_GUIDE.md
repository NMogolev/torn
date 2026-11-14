# Ask AI safely

Drawen is unusual enough that a model will guess the wrong API unless the prompt
sets boundaries. This page is the short, copyable contract. It is written for
both humans asking a model and agents editing a Drawen codebase.

## The one architecture

```text
application state -> widgets -> layout -> input -> paint -> DisplayList -> renderer
```

- Application code depends on the `drawen` facade. Add
  `drawen-platform-winit` only for a native window.
- Do not import `drawen-ui`, `drawen-widgets`, `drawen-core`, or
  `drawen-render` in ordinary application code. Use the facade re-exports.
- `UiRuntime` owns the retained widget tree. Build it with `append_child`; do not
  keep live widget values outside the runtime after moving them in.
- The tree, widgets, signals, and subscriptions are **single-threaded** and
  UI-thread-local. They are not `Send`/`Sync` and must not be touched from a
  worker thread. Run owned work on `drawen::tasks::TaskPool` and deliver owned
  data to the UI thread through `drawen::tasks::UiDispatcher`; mutate state in
  the event loop.
- Widgets do not call platform APIs, open windows, spawn threads, or start
  timers. They emit typed signals and use `EventContext` requests. The
  application or `drawen-platform-winit` performs the platform action.
- The native event loop is the one from `minimal_window`:
  `Resized -> layout`, `Input -> dispatch_event`, widget request -> redraw,
  `redraw -> paint`, `CloseRequested -> Exit`.

## Facts models commonly get wrong

| Wrong guess | What Drawen actually has |
| --- | --- |
| `Button::new("Save")` | `Button::new()` then append a `Text` child |
| `button.on_click(...)` | `button.activated().subscribe(...)`, keep the `Subscription` |
| `Text::new("hello")` | `Text::styled("hello", TextStyle::new(px, color))`, or `Text::new(prepared_layout)` |
| `use drawen::TextStyle` | `TextStyle` is in `drawen::render`; `TextLayout` and shapers are there too |
| `text.set_text("...")` | Rebuild the label from application state, or use `TextField::set_text` for editable text |
| React hooks, `use_state`, virtual DOM | Plain Rust state; retained widgets are updated explicitly |
| `Widget::new().build()` | Widgets are values appended with `UiRuntime::append_child` |
| CSS descendant selectors, animations | DSS compound selectors and typed properties only |
| `signal.connect` from any thread | `Signal::subscribe` on the UI thread, returns `drop`-based `Subscription` |
| `std::thread::spawn` that captures a widget | `drawen::tasks::TaskPool` + `UiDispatcher`; workers own plain data |
| An audio widget with its own timer | `Video` + `MediaSession`; audio is part of the media session |
| `drawen::MediaDirector` by default | Enable the `director` feature; `AudioPaleoSession` needs `audio` |

## Q&A

### "Make me a window with a button."

Copy [`minimal_window.rs`](../crates/drawen/examples/minimal_window.rs). The
essential shape is:

```rust
let mut runtime = UiRuntime::new(root);
let button = runtime.append_child(runtime.root(), Button::new())?;
runtime.append_child(button, Text::styled("Save", TextStyle::new(16.0, Color::BLACK)))?;
runtime.layout(Constraints::tight(size)?)?;
```

Then implement `WindowApplication`: `window_options`, `window_event`, and
`redraw`. Do not add a second event loop or a background thread that mutates the
runtime.

### "Why doesn't `Button::new()` take text?"

`Button` is a clickable single-child surface. The child is arbitrary content:
`Text`, `Row`, an icon, a spinner, or a composed widget. This is composition,
not an omission. Put labels in the child; put padding, border, hover, pressed,
and focus styling on the button.

### "Add application state and update the UI."

Use `Rc<RefCell<State>>` for shared model state, a widget signal to mutate it,
and one redraw path. The canonical runnable example is
[`stateful_app.rs`](../crates/drawen/examples/stateful_app.rs). A state signal
handler should set a dirty flag; the event loop calls `invalidate_paint()` and
returns `WindowAction::RequestRedraw`. Call `invalidate_layout()` only when
geometry changes.

Do not invent a reactive dependency graph, hooks, or a global store. A
`Signal<T>` is a synchronous notification channel, not an observable framework.

### "Can I update the UI from another thread?"

No. The widget tree is UI-thread-only. Run decoding, parsing, or computation on
a `drawen::tasks::TaskPool` worker, then deliver owned data through a
`drawen::tasks::UiDispatcher` and apply it from `window_event` (the native
adapter wakes the loop through `WindowApplication::attach_waker`),
`next_frame_in`, or a widget animation tick. Never hold a `UiRuntime`, `Widget`,
`Signal`, or `Subscription` across threads.

### "Add a stylesheet."

Use `UiEnvironment::from_dss(theme, source)` and
`UiRuntime::with_environment`. Attach identity with
`Styled::new(widget).with_id(...)` and `.with_class(...)`. The runtime adds
`:hover` and `:focus`. DSS is not full CSS: no descendant/sibling combinators,
variables, `calc()`, media queries, keyframes, or animations. See
[RECIPES.md](RECIPES.md#4-a-stylesheet-with-dss) and [DSS.md](DSS.md).

### "Play this MP4 and turn on the sound."

Choose the backend explicitly:

```rust
let video = Video::open_paleo("clip.mp4")?;        // pure Rust, H.264, no audio
let video = Video::open_paleo_audio("clip.mp4")?;  // requires feature "audio"
let video = Video::open("clip.mp4")?;              // external ffmpeg/ffprobe, no audio
```

Only `open_paleo_audio` has an audio-mastered clock. Audio continues only while
the event loop ticks the session; return `runtime.next_frame_in()` from
`WindowApplication::next_frame_in`. Use `drawen-director` with the `director`
feature for minimize/occlude/close policy. Background playback after the last
window closes requires
`MediaDirector::with_background_playback(true)`; it is deliberately not the
default.

### "Why is my window frozen or my audio stopped?"

Common causes:

- `UiRuntime::layout` is not called after a resize, so hit testing and painting
  use stale geometry.
- A `Subscription` was dropped immediately, so the click handler never runs.
- The event loop ignores `take_redraw_request()` or never returns a frame
  interval from `next_frame_in()`.
- A long decode or file read runs inside `paint`; paint must only record display
  commands. Move blocking work out of the UI thread.
- A worker thread tries to mutate the runtime; that is not supported.

## Prompt template

Paste this before asking a model for Drawen code:

```text
Use Drawen 0.1. Application code depends on the `drawen` facade, plus
`drawen-platform-winit` for a native window. Do not use drawen-ui or
drawen-widgets directly.

The widget tree, signals, and subscriptions are UI-thread-only. Widgets do not
spawn threads or timers and do not call platform APIs. Build with
`UiRuntime::append_child`, lay out with `Constraints`, dispatch `InputEvent`s,
paint to a `DisplayList`, and let `WindowApplication` return `WindowAction`s.

`Button::new()` takes no label: append a `Text` child. Signals use
`.subscribe(...)` and the returned `Subscription` must be kept alive. State is
plain Rust (`Rc<RefCell<_>>`); after a state change call `invalidate_paint()` and
return `WindowAction::RequestRedraw`. DSS has no descendant selectors,
animations, or full browser CSS.

Copy the event loop and media policy from:
- crates/drawen/examples/minimal_window.rs
- crates/drawen/examples/stateful_app.rs
- docs/RECIPES.md
- docs/MEDIA.md
```

## Canonical files to copy

- [`minimal_window.rs`](../crates/drawen/examples/minimal_window.rs) — native
  window, button, resize, close, redraw.
- [`stateful_app.rs`](../crates/drawen/examples/stateful_app.rs) — state,
  signal subscription, dirty paint, lifecycle.
- [`hello_drawen.rs`](../crates/drawen/examples/hello_drawen.rs) — a more
  complete single-window application.
- [`workspace.rs`](../crates/drawen/examples/workspace.rs) — docking, tabs,
  persistence.
- [`background_tasks.rs`](../crates/drawen/examples/background_tasks.rs) —
  worker pool, progress, and UI delivery.
- [`RECIPES.md`](RECIPES.md) — the seven application patterns.
- [`MEDIA.md`](MEDIA.md) — sessions, clocks, backends, lifecycle policy.
- [`THREADING.md`](THREADING.md) — task pools, cancellation, and the UI bridge.
