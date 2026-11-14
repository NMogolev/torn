# Drawen

**Drawen** is a Rust UI toolkit for native desktop applications. It combines a
retained widget tree, backend-neutral display lists, CSS-inspired stylesheets,
docking and workspaces, accessibility, and optional media and browser surfaces in
one workspace.

The toolkit is designed around composition: widgets implement traits, describe
layout and paint work, and remain independent of the renderer and windowing
system. The same tree can run headlessly in tests, render deterministically to a
pixel buffer, or be presented through `wgpu` in a native `winit` window.

## Quick start

### Headless: build, lay out, paint, write a PNG

Add the facade crate:

```text
cargo add drawen
```

Create a widget tree, lay it out, paint it, and write the result to a PNG:

```rust
use drawen::{
    Box as DrawenBox, Button, Color, Constraints, Size, Text, UiRuntime,
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
    software::{PixelBuffer, SoftwareRenderer},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let label = Text::new(FontdueTextShaper::ubuntu_light().layout(
        "Hello, Drawen",
        &TextStyle::new(16.0, Color::BLACK),
        None,
    ));

    let mut root = DrawenBox::new();
    root.set_background(Some(Color::WHITE));

    let mut runtime = UiRuntime::new(root);
    let button = runtime.append_child(runtime.root(), Button::new())?;
    runtime.append_child(button, label)?;
    runtime.layout(Constraints::tight(Size::new(320.0, 180.0)?)?)?;

    let mut display_list = DisplayList::new();
    runtime.paint(&mut PaintContext::new(&mut display_list))?;

    let mut pixels = PixelBuffer::new(320, 180)?;
    SoftwareRenderer.render(&display_list, &mut pixels)?;
    pixels.write_png("target/drawen.png")?;
    Ok(())
}
```

Run the complete example from a workspace checkout:

```text
cargo run -p drawen --example headless_tutorial
```

The headless path needs no display server, GPU, or audio device. That is why it
is the first example: it makes the retained tree and display list visible
without hiding them behind platform setup.

### Native window: the same tree in one event loop

Native applications use the same widget tree and pass it to a platform adapter.
Add the `winit` adapter as well:

```text
cargo add drawen drawen-platform-winit
```

A complete window with a button and the canonical event loop. The runnable
`minimal_window` example is the same code with comments and a small resize
helper:

```rust
use drawen::{
    Box as DrawenBox, Button, Color, Constraints, Size, Subscription, Text, UiRuntime,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, PaintContext, TextStyle},
};

struct App { ui: UiRuntime, size: Size, _clicks: Subscription }

impl WindowApplication for App {
    fn window_options(&self) -> WindowOptions { WindowOptions::new("Drawen", self.size) }
    fn window_event(&mut self, event: WindowEvent) -> WindowAction {
        match event {
            WindowEvent::CloseRequested => WindowAction::Exit,
            WindowEvent::Resized(size) => {
                self.size = size;
                if let Ok(c) = Constraints::tight(size) { let _ = self.ui.layout(c); }
                WindowAction::RequestRedraw
            }
            WindowEvent::Input(event) => {
                let _ = self.ui.dispatch_event(&event);
                if self.ui.take_redraw_request() {
                    WindowAction::RequestRedraw
                } else {
                    WindowAction::None
                }
            }
            _ => WindowAction::None,
        }
    }
    fn redraw(&mut self) -> DisplayList {
        let mut list = DisplayList::new();
        let _ = self.ui.paint(&mut PaintContext::new(&mut list));
        list
    }
}

fn main() -> Result<(), drawen_platform_winit::RunError> {
    let size = Size::new(480.0, 280.0).expect("valid size");
    let mut root = DrawenBox::new();
    root.set_background(Some(Color::WHITE));
    let mut ui = UiRuntime::new(root);
    let button = Button::new();
    let clicks = button.activated().subscribe(|()| println!("Clicked"));
    let button = ui.append_child(ui.root(), button).expect("root exists");
    ui.append_child(button, Text::styled("Click me", TextStyle::new(16.0, Color::BLACK)))
        .expect("button exists");
    ui.layout(Constraints::tight(size).expect("valid constraints")).expect("layout");
    drawen_platform_winit::run(App { ui, size, _clicks: clicks })
}
```

```text
cargo run -p drawen --example minimal_window
```

This is the single correct event-loop shape: resize means **layout**, input
means **dispatch**, a widget request means **request redraw**, and `redraw`
means **paint**. Widgets never create a second window, event loop, timer thread,
or platform handle. `hello_drawen` extends this shape with lifecycle events and
timing; `stateful_app` adds application state.

## Features

- **Retained widget tree.** Stable widget nodes own layout, paint, focus, and
  invalidation state. Damage and dirty state are tracked per node.
- **Capture, target, and bubble events.** Pointer capture, focus, keyboard
  navigation, and local widget signals behave consistently across backends.
- **Typed signals.** Widgets emit typed, UI-thread-local signals with drop-based
  subscriptions instead of a global dynamic event bus.
- **Backend-neutral painting.** Widgets record a `DisplayList`; the deterministic
  software renderer and the `wgpu` renderer execute the same commands.
- **Background work.** A fixed task pool with cooperative cancellation,
  progress, and a wakeable UI dispatcher keeps decoding and parsing off the UI
  thread without touching widgets from a worker.
- **Production text.** HarfRust shaping, bidirectional layout, grapheme-aware
  fallback, system font discovery, wrapping, caret hit testing, and selection
  geometry.
- **DSS stylesheets.** A small CSS-compatible cascade with type, id, class, and
  state selectors resolves into typed style values.
- **Application building blocks.** Buttons, text fields, editors, lists, virtual
  tables, menus, toolbars, tabs, splitters, scrolling, cards, alerts, progress
  controls, and more.
- **Docking and workspaces.** A serializable layout model with split panels,
  tabs, floating windows, MDI documents, and versioned JSON persistence.
- **Accessibility.** Semantic roles, states, actions, and an AccessKit bridge.
- **Media and browser surfaces.** In-process Paleo playback for H.264 and audio,
  an optional FFmpeg command-line backend, and Servo shared-texture embedding.

## Common recipes

The canonical snippets live in
[`docs/RECIPES.md`](https://github.com/NMogolev/drawen/blob/main/docs/RECIPES.md).
Start there instead of inventing a second architecture:

- **Button with text** — `Button` is a single-child surface; append a `Text`.
- **Form and `TextField`** — hold the submitted value in application state and
  subscribe to `changed()`.
- **Application state and signals** — plain Rust state, one signal subscription,
  one paint invalidation. See `stateful_app`.
- **Stylesheet** — `UiEnvironment::from_dss`, `Styled`, and runtime-owned
  `:hover`/`:focus`.
- **Scrolling and virtual lists** — `ScrollView`, `ListView`, `VirtualList`.
- **Docking workspace** — `WorkspaceLayout` plus `DockArea`.
- **Video and audio** — `Video::open_paleo`, `open_paleo_audio`, and the FFmpeg
  command-line path.

### Why `Button::new()` takes no text

`Button` is a clickable surface with exactly one runtime-owned child. It accepts
any content, not only a label. The canonical form is:

```rust
let button = runtime.append_child(runtime.root(), Button::new())?;
runtime.append_child(button, Text::styled("Save", TextStyle::new(14.0, Color::BLACK)))?;
```

This is why there is no `Button::new("Save")`: a button can contain a `Text`, a
`Row` with an icon, a spinner, or a composed custom widget. Appearance belongs to
the button; content belongs to its child.

## Workspace

Applications normally depend on the `drawen` facade crate. The lower-level crates
remain available for headless tests, custom backends, and smaller binaries.

| Crate | Purpose |
| --- | --- |
| [`drawen`](https://crates.io/crates/drawen) | Public facade: runtime, widgets, renderers, platform contracts |
| [`drawen-core`](https://crates.io/crates/drawen-core) | Geometry, constraints, colors, input data, and IDs |
| [`drawen-render`](https://crates.io/crates/drawen-render) | Display lists, paint contracts, fonts, and text shaping |
| [`drawen-ui`](https://crates.io/crates/drawen-ui) | Retained tree, layout lifecycle, event routing, themes, DSS |
| [`drawen-widgets`](https://crates.io/crates/drawen-widgets) | Standard layout and control widgets |
| [`drawen-software`](https://crates.io/crates/drawen-software) | Deterministic software renderer and test support |
| [`drawen-tasks`](https://crates.io/crates/drawen-tasks) | Worker pool, cancellation, progress, and the UI-thread dispatcher |
| [`drawen-wgpu`](https://crates.io/crates/drawen-wgpu) | GPU display-list renderer |
| [`drawen-platform`](https://crates.io/crates/drawen-platform) | Platform-independent window and event contracts |
| [`drawen-platform-winit`](https://crates.io/crates/drawen-platform-winit) | Native `winit` window adapter |
| [`drawen-workspace`](https://crates.io/crates/drawen-workspace) | Persistent docking and document layouts |
| [`drawen-director`](https://crates.io/crates/drawen-director) | Optional media lifecycle and presentation policy |
| [`drawen-browser`](https://crates.io/crates/drawen-browser) | Browser surface and Servo integration layer |
| [`drawen-servo`](https://crates.io/crates/drawen-servo) | Platform selection for Servo shared-texture interop |
| [`drawen-servo-windows`](https://crates.io/crates/drawen-servo-windows) | Windows DXGI shared-texture interop |
| [`drawen-servo-linux`](https://crates.io/crates/drawen-servo-linux) | Linux Vulkan dmabuf/EGL shared-texture interop |
| [`drawen-servo-macos`](https://crates.io/crates/drawen-servo-macos) | macOS IOSurface shared-texture interop |
| [`paleo`](https://crates.io/crates/paleo) | Pure-Rust container demuxing and audio decoding |
| [`paleo-h264`](https://crates.io/crates/paleo-h264) | Pure-Rust H.264 decoding backend |
| [`paleo-audio`](https://crates.io/crates/paleo-audio) | Audio output, PCM conversion, and playback clock |

## Optional features

The facade exposes two optional features:

```text
cargo add drawen --features audio,director
```

- `audio` enables `AudioPaleoSession`, audio-mastered playback, and the platform
  output device through `cpal`.
- `director` enables `MediaDirector`, which maps window lifecycle events to
  playback policy.

The FFmpeg command-line backend is always available through `Video::open`; it
does not add a build feature. It does require external `ffmpeg` and `ffprobe`
executables at run time.

## Media capabilities

There is no single "media player" switch. Pick the path that matches the file,
the platform, and whether audio must be real.

| Entry point | Feature | Video | Audio | External requirements | Important limits |
| --- | --- | --- | --- | --- | --- |
| `Video::open_paleo` | none | H.264, 4:2:0 frame pictures | none | None; pure Rust and in-process | H.264 only; bounded synchronous decode on the calling thread |
| `Video::open_paleo_audio` | `audio` | H.264 when present | AAC-LC, ALAC, FLAC, MP1/MP2/MP3, Vorbis, PCM, ADPCM | A default output device through `cpal` | Audio is the master clock; playback is fixed at `1x`; no hot-plug failover |
| `Video::open` | none | Whatever the local `ffmpeg` decodes | none | `ffmpeg` and `ffprobe` on `PATH` or in `FfmpegConfig` | Out-of-process, raw RGBA frames, 1280 px default cap, CFR timestamps, no audio |
| `Video::with_session` | depends on session | Custom `MediaSession` | depends on session | Application-supplied | The widget only presents frames and forwards transport intent |

Paleo demuxes more codecs than it decodes: HEVC, AV1, VP8/VP9, and MPEG-4 Part 2
packets are recognized, but only H.264 has a decoder today. Audio decoding does
not include HE-AAC/HE-AACv2 or Opus yet. See
[MEDIA.md](https://github.com/NMogolev/drawen/blob/main/docs/MEDIA.md) and
[PALEO.md](https://github.com/NMogolev/drawen/blob/main/docs/PALEO.md) for the
contract and backend details.

## Background playback

The default desktop behavior is: minimizing or occluding a window keeps audio
playing and stops video presentation; closing the last window exits. Background
playback after the last window closes is an explicit product decision. The
snippet requires the `director` feature; pair it with `audio` for
`Video::open_paleo_audio`:

```rust
use drawen::{
    MediaDirector, Video, VisibilityPolicy,
    platform::{WindowAction, WindowEvent},
};

let mut director = MediaDirector::new()
    .with_policy(VisibilityPolicy::PauseVideoKeepAudio)
    .with_background_playback(true); // opt in: do not exit on last close

fn handle(
    director: &mut MediaDirector,
    video: &mut Video,
    event: &WindowEvent,
) -> WindowAction {
    let Some(action) = director.window_event(event) else {
        return WindowAction::None;
    };
    director.apply(action, video);
    action.window()
}
```

`PauseVideoKeepAudio` is already the default visibility policy; set it explicitly
when the behavior is part of the product contract. The director does **not**
create a timer. Audio continues only while the event loop keeps ticking the
`MediaSession`; a fully hidden window may stop receiving redraws, so a
background-playing application must keep a tick source that does not depend on
painting. See
[MEDIA.md](https://github.com/NMogolev/drawen/blob/main/docs/MEDIA.md#audio-mastered-paleo-backend).

## Current limits

Drawen is usable today, but it is not a browser engine and not a universal media
player. The limits that surprise new applications most often:

**UI and styling**

- DSS is a deliberately small typed subset: type, id, class, and state
  selectors. There are no descendant or sibling combinators, variables,
  `calc()`, media queries, keyframes, animations, or full browser-CSS
  compatibility. Surface properties do not inherit.
- There is no HTML/CSS DOM or browser layout engine. `drawen-browser` is a
  shared-texture surface and hand-off contract; the actual Servo adapter is
  supplied by the application.
- Widgets are retained and single-threaded. They do not call platform APIs
  directly and do not own timers or worker threads.

**Media**

- Paleo video is H.264 4:2:0 only. HEVC, AV1, VP8/VP9, and MPEG-4 Part 2 still
  need decoder backends.
- Paleo audio does not decode HE-AAC, HE-AACv2, or Opus.
- `AudioPaleoSession` plays at `1x`, decodes on the calling thread, and needs an
  output device. Device loss is reported as an error instead of switching
  outputs automatically.
- The FFmpeg path needs external executables, has no audio track, no hardware
  decode, no zero-copy external textures, and no network or adaptive streaming.
- Multiple tracks, subtitles, chapters, adaptive bitrate, and encoding are not
  implemented.

**Platforms**

- Windows and Linux are the primary tested targets. macOS compiles through the
  same contracts but receives less day-to-day testing.
- Native windows need `winit`; GPU presentation needs `wgpu`; audio needs
  `cpal`; browser surfaces need the matching platform-specific Servo interop
  crate.
- The headless runtime, software renderer, widgets, and text stack remain
  portable and platform-independent.

## Examples

Run the bundled examples from a workspace checkout:

```text
cargo run -p drawen --example headless_tutorial
cargo run -p drawen --example minimal_window
cargo run -p drawen --example stateful_app
cargo run -p drawen --example hello_drawen
cargo run -p drawen --example style_showcase
cargo run -p drawen --example workspace
cargo run -p drawen --example calculator
cargo run -p drawen --example custom_titlebar
```

`headless_tutorial` and `style_showcase` render without opening a window.
`minimal_window`, `stateful_app`, `hello_drawen`, `calculator`, and
`custom_titlebar` use the native `winit` adapter. `workspace` demonstrates
docking, tabs, floating panels, and persisted layout data.

## Documentation

- [Common recipes](https://github.com/NMogolev/drawen/blob/main/docs/RECIPES.md)
  for copyable patterns: buttons, forms, state, stylesheets, lists, docking, and
  media.
- [Tutorial: build, lay out, paint, and test a Drawen UI](https://github.com/NMogolev/drawen/blob/main/docs/TUTORIAL.md)
- [Fonts and text shaping](https://github.com/NMogolev/drawen/blob/main/docs/FONTS.md)
- [Drawen Style Sheets (DSS)](https://github.com/NMogolev/drawen/blob/main/docs/DSS.md)
- [Docking, MDI, and persisted workspaces](https://github.com/NMogolev/drawen/blob/main/docs/DOCKING.md)
- [Custom window chrome](https://github.com/NMogolev/drawen/blob/main/docs/WINDOW_CHROME.md)
- [Media pipeline and backends](https://github.com/NMogolev/drawen/blob/main/docs/MEDIA.md)
- [Paleo media layer](https://github.com/NMogolev/drawen/blob/main/docs/PALEO.md)
- [Background work and threading](https://github.com/NMogolev/drawen/blob/main/docs/THREADING.md)
- [Using an AI assistant with Drawen](https://github.com/NMogolev/drawen/blob/main/docs/AI_GUIDE.md)

API documentation is published on [docs.rs/drawen](https://docs.rs/drawen).

## Using an AI assistant

If you ask a model to generate Drawen code, give it the constraints up front.
The short version:

- Application code depends on `drawen`, plus `drawen-platform-winit` for a
  native window. Do not reach into `drawen-ui` or `drawen-widgets` without a
  concrete lower-level reason.
- The widget tree, signals, and subscriptions are **UI-thread only**. Do not
  move them to another thread or touch them from a worker.
- Widgets do not call platform APIs and do not own timers or threads. Use typed
  signals, `EventContext` redraw/layout requests, `Widget::advance_animation`,
  and `UiRuntime::next_frame_in`.
- Copy the event-loop shape from `minimal_window` or `stateful_app`. Do not
  invent a second event loop, a React-style hook system, or a virtual DOM.
- `Button::new()` takes no label; append a `Text` child. DSS has no descendant
  selectors or animations.

The full checklist and a prompt template are in
[`docs/AI_GUIDE.md`](https://github.com/NMogolev/drawen/blob/main/docs/AI_GUIDE.md).
Repository agents also have a short pointer in
[`AGENTS.md`](https://github.com/NMogolev/drawen/blob/main/AGENTS.md).

## Platform support

The headless runtime, software renderer, widgets, and text stack are portable
Rust and have no platform dependency. Native windows, GPU rendering, audio
output, and browser surfaces are provided by adapters:

| Platform | Native windows | GPU | Audio | Browser surfaces |
| --- | --- | --- | --- | --- |
| Windows | `winit` | `wgpu` (DX12/Vulkan) | `cpal` | DXGI shared textures |
| Linux | `winit` (X11/Wayland) | `wgpu` (Vulkan) | `cpal` | Vulkan dmabuf/EGL |
| macOS | `winit` | `wgpu` (Metal) | `cpal` | IOSurface/Metal |

Windows and Linux are the primary tested targets. macOS support is compiled
through the same contracts but receives less day-to-day testing.

## Requirements

- Rust 1.88 or newer (edition 2024).
- A C toolchain is not required for the core workspace. Platform audio and GPU
  backends follow their system libraries' requirements.

## Development

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

Some media tests are opt-in because they need external tools or devices:

- `DRAWEN_FFMPEG` enables tests that generate and compare media fixtures.
- `DRAWEN_AUDIO_DEVICE_TEST` enables output-device smoke tests.

## License

MIT. See [LICENSE](https://github.com/NMogolev/drawen/blob/main/LICENSE).
