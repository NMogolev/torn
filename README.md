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

Native applications normally build the same tree and pass it to the `winit`
adapter instead of rendering to a `PixelBuffer`.

## Features

- **Retained widget tree.** Stable widget nodes own layout, paint, focus, and
  invalidation state. Damage and dirty state are tracked per node.
- **Capture, target, and bubble events.** Pointer capture, focus, keyboard
  navigation, and local widget signals behave consistently across backends.
- **Typed signals.** Widgets emit typed, UI-thread-local signals with drop-based
  subscriptions instead of a global dynamic event bus.
- **Backend-neutral painting.** Widgets record a `DisplayList`; the deterministic
  software renderer and the `wgpu` renderer execute the same commands.
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

- `audio` enables audio-mastered Paleo playback through the platform output
  device.
- `director` enables `MediaDirector`, which maps window lifecycle events to
  playback policy.

## Examples

Run the bundled examples from a workspace checkout:

```text
cargo run -p drawen --example headless_tutorial
cargo run -p drawen --example hello_drawen
cargo run -p drawen --example style_showcase
cargo run -p drawen --example workspace
cargo run -p drawen --example calculator
cargo run -p drawen --example custom_titlebar
```

`headless_tutorial` and `style_showcase` render without opening a window.
`hello_drawen`, `calculator`, and `custom_titlebar` use the native `winit`
adapter. `workspace` demonstrates docking, tabs, floating panels, and persisted
layout data.

## Documentation

- [Tutorial: build, lay out, paint, and test a Drawen UI](https://github.com/NMogolev/drawen/blob/main/docs/TUTORIAL.md)
- [Fonts and text shaping](https://github.com/NMogolev/drawen/blob/main/docs/FONTS.md)
- [Drawen Style Sheets (DSS)](https://github.com/NMogolev/drawen/blob/main/docs/DSS.md)
- [Docking, MDI, and persisted workspaces](https://github.com/NMogolev/drawen/blob/main/docs/DOCKING.md)
- [Custom window chrome](https://github.com/NMogolev/drawen/blob/main/docs/WINDOW_CHROME.md)
- [Media pipeline and backends](https://github.com/NMogolev/drawen/blob/main/docs/MEDIA.md)
- [Paleo media layer](https://github.com/NMogolev/drawen/blob/main/docs/PALEO.md)

API documentation is published on [docs.rs/drawen](https://docs.rs/drawen).

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
