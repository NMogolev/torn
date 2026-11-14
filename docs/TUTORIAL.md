# Tutorial

This document walks through the API that is implemented in Drawen today. An
application builds a widget tree, lays it out, records paint commands in a
`DisplayList`, dispatches input events, and can either export the result as a
PNG or present it in a native window.

The complete runnable headless source is
[`crates/drawen/examples/headless_tutorial.rs`](../crates/drawen/examples/headless_tutorial.rs).

## Quick start

From the repository root:

```text
cargo run -p drawen --example headless_tutorial
```

The example prints the number of completed clicks and writes
`target/drawen-tutorial.png`.

For a native window on Windows or Linux:

```text
cargo run -p drawen --example hello_drawen
```

The windowed example is built on `winit`: Win32 on Windows, and X11 or Wayland
on Linux. A native window remains optional. The core crates and headless tests
do not depend on platform code.

For a window with custom chrome and minimize, maximize, and close controls:

```text
cargo run -p drawen --example custom_titlebar
```

See [WINDOW_CHROME.md](WINDOW_CHROME.md) for the title-bar contract and
platform differences.

## 1. Imports through the facade

Application code normally depends on the `drawen` crate, not on `drawen-core`,
`drawen-ui`, or `drawen-widgets` individually:

```rust
use drawen::{
    Box as DrawenBox, Button, Color, Constraints, LightTheme, Point, Size, Text, UiEnvironment,
    UiRuntime,
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
    software::{PixelBuffer, SoftwareRenderer},
};
```

The main types (`Box`, `Button`, `Text`, `Row`, `Column`, `UiRuntime`,
`Constraints`) are re-exported from the crate root. Lower-level contracts live
in modules:

- `drawen::render` — `DisplayList`, `PaintContext`, `TextLayout`, `TextShaper`,
  and fonts;
- `drawen::software` — `SoftwareRenderer`, `PixelBuffer`, and PNG export;
- `drawen::gpu` — the `wgpu` display-list renderer;
- `drawen::platform` — native-window contracts and intent translation;
- `drawen::browser` — Servo shared-texture surfaces, where supported.

## 2. Text

`Text` takes a prepared `TextLayout`, not a raw string. The deterministic
bundled shaper works with a font database and supports families, weights, styles,
fallback, complex shaping, and bidirectional text:

```rust
use drawen::{Color, Font, FontSize, FontWeight};
use drawen::render::{FontdueTextShaper, TextStyle};

let font = Font::new()
    .with_family("Ubuntu")
    .with_size(FontSize::pixels(16.0))
    .with_weight(FontWeight::LIGHT);
let label = Text::new(FontdueTextShaper::ubuntu_light().layout(
    "Click the button",
    &TextStyle::new_with_font(font, Color::BLACK),
    None,
));
```

Shaping and painting are separate responsibilities: the shaper measures and
prepares the text, and the widget uses the known size during layout.
`TextLayout` stores the source UTF-8 text, the resolved faces, glyph runs,
baselines, and line metrics, so measurement and painting use the same data.

When text should follow the application font and the node's DSS properties, use
`Text::styled`. The widget defers shaping until layout and takes the shaper from
the `UiEnvironment`:

```rust
let label = Text::styled("Click the button", TextStyle::new(16.0, Color::BLACK));
```

`UiEnvironment::default_font()` plays the role of `QApplication::font()`, and
`font_for(&style)` applies the node's `font-family`, `font-size`,
`font-weight`, and `font-style`. See [FONTS.md](FONTS.md) for the request/face
model and fallback rules.

`SoftwareRenderer` rasterizes glyph coverage and respects the active clip
rectangle. The bundled text stack performs HarfRust OpenType shaping, kerning,
ligatures, mark positioning, bidirectional reordering, grapheme-aware font
fallback, newline handling, width wrapping, caret hit testing, and selection
geometry.

## 3. Buttons and boxes

`UiRuntime` owns the widget tree. `Button` is a clickable single-child control.
It insets its child by the theme's control padding and handles primary pointer
events and keyboard activation:

```rust
let mut button = Button::new();
button.set_backgrounds(
    Color::rgba8(180, 220, 255, 255),
    Color::rgba8(120, 180, 230, 255),
);
let subscription = button.activated().subscribe(|()| println!("Clicked"));
```

`activated()` returns a typed `Signal<()>`. The click completes on `PointerUp`
inside the button, and the button captures the pointer so a drag that starts
inside it keeps receiving events. Keep the returned `Subscription` alive for as
long as the handler should fire.

`Box` is closer to a minimal `div`: it can contain one child and paint a
background. The runtime creates parent/child links; the outer `Box` in the
example provides a white canvas:

```rust
let mut root = DrawenBox::new();
root.set_background(Some(Color::WHITE));
```

## 4. Layout and painting

`UiRuntime` owns the root widget. Call `layout` before dispatching events or
painting:

```rust
let mut runtime = UiRuntime::with_environment(root, UiEnvironment::new(LightTheme));
let button = runtime.append_child(runtime.root(), button)?;
runtime.append_child(button, label)?;
let canvas = Size::new(320.0, 180.0)?;
runtime.layout(Constraints::tight(canvas)?)?;

let mut display_list = DisplayList::new();
runtime.paint(&mut PaintContext::new(&mut display_list))?;
```

`UiEnvironment` belongs to the runtime and is available to every widget during
layout and paint. It contains the theme, scale factor, locale, the default font,
and the text shaper. A standard `Button` resolves its normal and pressed colors
from the `Theme`; `set_backgrounds` remains a local override. For the standard
light palette, `UiRuntime::new(root)` is a shorthand for a runtime with the
default environment.

`Constraints::tight(canvas)` gives the root an exact canvas size. A `Box` keeps
its child at the top-left corner but stretches its background to the final size.

The frame order is:

```text
widget state -> layout -> InputEvent -> paint -> DisplayList -> renderer
```

If application code changes widget state through `UiRuntime::root_mut()` or
another mutable accessor, the retained layout is invalidated automatically. Call
`layout` again before the next `paint` or `dispatch_event`.

## 5. Dispatching a click

Events use coordinates relative to the root. The runtime performs hit testing and
passes local coordinates to each widget:

```rust
use drawen::{
    InputEvent, Modifiers, Point, PointerButton, PointerButtons, PointerEvent, PointerId,
};

let event = InputEvent::PointerDown(PointerEvent {
    pointer_id: PointerId(1),
    position: Point::new(12.0, 12.0),
    button: Some(PointerButton::Primary),
    buttons: PointerButtons::PRIMARY,
    modifiers: Modifiers::NONE,
});

let result = runtime.dispatch_event(&event);
assert!(result.is_handled());
```

The router runs every event through `capture -> target -> bubble`. Pointer and
wheel coordinates are local to the current widget in each phase.
`EventContext` can stop propagation, request focus, capture a pointer, and request
a new frame. A captured pointer keeps delivering move and up events to the
original widget outside its bounds. `KeyDown`, `KeyUp`, and `TextInput` are
routed to the focused widget.

## 6. Diagnostics instead of opaque failures

`UiRuntime` isolates panics in application implementations of `Widget::layout`,
`Widget::paint`, and `Widget::handle_event`. Instead of propagating a panic,
`layout` and `paint` return `UiRuntimeError::WidgetPanicked`, and the runtime
retains a structured `Diagnostic`:

```rust
match runtime.layout(Constraints::tight(canvas)?) {
    Ok(_) => {}
    Err(error) => {
        for diagnostic in runtime.take_diagnostics() {
            eprintln!("{diagnostic}");
        }
        return Err(error.into());
    }
}
```

Diagnostics are collected inside the runtime by default. To forward them to an
application logger at the same time, install a reporter:

```rust
runtime.set_diagnostic_reporter(|diagnostic: &drawen::Diagnostic| {
    eprintln!("{diagnostic}");
});
```

In strict tests, the `PanicOnDiagnostic` reporter turns any diagnostic into a
panic, so a CI job does not miss a framework error:

```rust
runtime.set_diagnostic_reporter(drawen::PanicOnDiagnostic);
```

`SoftwareRenderer` keeps a regular `Result` API and also provides
`render_with_diagnostics`. It reports display-list errors and warns when it
cannot paint a command:

```rust
let mut diagnostics = Vec::new();
SoftwareRenderer.render_with_diagnostics(&display_list, &mut pixels, &mut diagnostics)?;
```

## 7. Rendering to PNG

`SoftwareRenderer` executes `FillRect`, rounded surfaces, borders, clip
operations, transforms, rasterized text, and source-over composition into a
`PixelBuffer`. `PixelBuffer::write_png` then writes a deterministic RGBA PNG:

```rust
let mut pixels = PixelBuffer::new(320, 180)?;
SoftwareRenderer.render(&display_list, &mut pixels)?;
pixels.write_png("target/drawen-tutorial.png")?;
```

## Further reading

- [DSS](DSS.md) for stylesheets, themes, and per-node styling.
- [Fonts](FONTS.md) for font requests, fallback, shaping, and system fonts.
- [Window chrome](WINDOW_CHROME.md) for custom title bars and native window
  intents.
- [Docking](DOCKING.md) for persisted workspaces and MDI documents.
- [Media](MEDIA.md) and [Paleo](PALEO.md) for video and audio playback.
