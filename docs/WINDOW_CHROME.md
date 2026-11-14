# Custom Window Chrome

Drawen can draw its own title bar and window controls instead of relying on the
operating system frame. The window remains a normal native window: it can still
be moved, resized, minimized, maximized, made fullscreen, and closed.

The feature is built from three layers:

- `drawen-platform` describes window options, window events, and window actions.
- `drawen-ui` carries widget requests as `WindowCommand` intents.
- `drawen-widgets` provides `TitleBar` and `WindowFrame` for painting and input.

Widgets never call a native windowing API directly. They request an intent, the
host application drains it, and the host maps it to its platform adapter. This
keeps widgets portable and testable.

## 1. Create an undecorated window

Set `decorations` to `false` in `WindowOptions`:

```rust
fn window_options(&self) -> WindowOptions {
    WindowOptions::new("My Application", self.size)
        .with_decorations(false)
        .with_resizable(true)
        .with_min_size(Size::new(640.0, 420.0).expect("valid minimum size"))
}
```

Other builders control the initial state:

| Builder | Effect |
| --- | --- |
| `with_decorations(bool)` | Show or hide the native frame. |
| `with_resizable(bool)` | Allow or reject user resizing. |
| `with_maximized(bool)` | Start maximized. |
| `with_fullscreen(bool)` | Start in borderless fullscreen. |
| `with_min_size(Size)` | Set the minimum content size. |

The defaults keep native decorations, allow resizing, start in windowed mode,
and do not set a minimum size, so existing applications are unaffected.

## 2. Build the frame and title bar

`WindowFrame` is the root shell. Its first child is placed in the title-bar
slot, its second child fills the content area, and any further children are
ignored. Edge and corner presses request a native resize before the event
reaches the title bar or content.

```rust
use drawen::{
    Border, Color, TitleBar, TitleBarStyle, WindowChromeState, WindowControl,
    WindowFrame,
};

let chrome = WindowChromeState::new();

let title_bar = TitleBar::new("My Application")
    .with_icon("◆")
    .with_subtitle("  custom chrome")
    .with_controls([
        WindowControl::Minimize,
        WindowControl::Maximize,
        WindowControl::ToggleFullscreen,
        WindowControl::Close,
    ])
    .with_state(chrome.clone())
    .with_style(TitleBarStyle {
        height: 44.0,
        background: Some(Color::rgba(0.09, 0.10, 0.13, 1.0)),
        foreground: Some(Color::WHITE),
        close_hover_background: Some(Color::rgba(0.86, 0.24, 0.28, 1.0)),
        border: Some(Border::new(1.0, Color::rgba(0.20, 0.22, 0.27, 1.0))),
        ..TitleBarStyle::default()
    });

let frame = WindowFrame::new()
    .with_title_bar_height(44.0)
    .with_resize_border(7.0);

let mut runtime = UiRuntime::new(frame);
let root = runtime.root();
runtime.append_child(root, title_bar).expect("frame exists");
runtime.append_child(root, content).expect("frame exists");
```

### Title bar style

`TitleBarStyle` is a plain struct, so applications can start from
`TitleBarStyle::default()` and override only the fields they care about.

| Field | Purpose |
| --- | --- |
| `height` | Preferred bar height in logical pixels. |
| `padding` | Insets around the title block and controls. |
| `title_alignment` | Start, center, or end alignment of the title slot. |
| `controls_alignment` | Side that owns the window controls. |
| `button_width` | Width of each window-control button. |
| `button_spacing` | Gap between adjacent controls. |
| `icon_gap` | Gap between icon, title, subtitle, and controls. |
| `show_title` / `show_icon` | Toggle the built-in title block. |
| `draggable` | Start a native window drag from the empty bar area. |
| `double_click_maximizes` | Toggle maximize on a double click. |
| `background`, `foreground`, `muted_foreground` | Base colors. |
| `hover_background`, `pressed_background` | Non-close control states. |
| `close_hover_background`, `close_pressed_background` | Close control states. |
| `corner_radius`, `border` | Surface decoration. |

Every color that is left as `None` resolves from the semantic theme:
`title_bar_background`, `title_bar_foreground`, `title_bar_hover_background`,
`muted_foreground`, `danger`, and the regular control colors.

The builder methods (`with_background`, `with_height`, `with_controls`, and so
on) are convenience wrappers over the same style fields.

### Shared maximized/fullscreen state

`WindowChromeState` is cheap to clone and shared through `Rc<Cell<bool>>`. Pass
one clone to the title bar and keep another in the application:

```rust
WindowEvent::Maximized(maximized) => {
    self.chrome.set_maximized(maximized);
    WindowAction::RequestRedraw
}
WindowEvent::Fullscreen(fullscreen) => {
    self.chrome.set_fullscreen(fullscreen);
    WindowAction::RequestRedraw
}
```

The title bar updates the state immediately when the user clicks a control, so
the maximize/restore glyph changes without waiting for the next native event.
The native event then confirms the final state.

### Custom title content

Append at most one child to `TitleBar` to replace the built-in title block. The
child is laid out in the free space between the leading and trailing controls,
which is useful for tabs, a search field, or a document name:

```rust
let title_bar = TitleBar::new("").with_style(TitleBarStyle {
    show_title: false,
    ..TitleBarStyle::default()
});
let title_bar = runtime.append_child(root, title_bar).expect("frame exists");
runtime.append_child(title_bar, my_tabs).expect("title bar exists");
```

## 3. Forward window intents

After dispatching an input event, drain the widget requests and translate them
with `drawen::platform::action_for`:

```rust
WindowEvent::Input(event) => {
    let _ = self.runtime.dispatch_event(&event);

    let mut action = WindowAction::None;
    for command in self.runtime.take_window_commands() {
        action = action_for(command);
    }

    if action == WindowAction::None && self.runtime.take_redraw_request() {
        action = WindowAction::RequestRedraw;
    }
    action
}
```

The available commands are:

| `WindowCommand` | `WindowAction` |
| --- | --- |
| `StartDrag` | `StartDrag` |
| `Minimize` | `Minimize` |
| `ToggleMaximize` | `ToggleMaximize` |
| `ToggleFullscreen` | `ToggleFullscreen` |
| `Close` | `Exit` |
| `StartResize(edge)` | `StartResize(direction)` |

`WindowFrame` emits `StartResize` from all eight edges and corners. `TitleBar`
emits `StartDrag` from the empty bar area, `ToggleMaximize` on a double click,
and one control command on each control release.

If the application needs a different close policy, handle `WindowCommand::Close`
itself instead of mapping it to `WindowAction::Exit`.

## 4. Platform behaviour

The winit adapter implements the actions with `drag_window`,
`drag_resize_window`, `set_minimized`, `set_maximized`, and `set_fullscreen`.

- **Windows.** Undecorated windows request a native drop shadow so they keep a
  visible boundary. `drag_window` and `drag_resize_window` use the standard
  Win32 move/size loops.
- **Linux (X11 and Wayland).** Moving and resizing use the compositor's
  interactive move/resize protocols. Most compositors support them, but a few
  tiling or restricted Wayland compositors may ignore interactive move.
- **macOS.** winit supports the same operations, but native macOS conventions
  differ: window controls belong on the left, the green button normally toggles
  fullscreen rather than maximize, and traffic lights have their own spacing.
  If a native look matters more than a uniform cross-platform bar, keep
  `decorations(true)` on macOS and disable the custom title bar there. The
  widgets themselves do not depend on the platform and can still be used for
  in-window headers.

Borderless fullscreen uses `Fullscreen::Borderless(None)`, so the window covers
the current monitor without changing video mode.

## 5. Complete example

[`crates/drawen/examples/custom_titlebar.rs`](../crates/drawen/examples/custom_titlebar.rs)
combines `WindowFrame`, `TitleBar`, `WindowChromeState`, `Switch`, `Badge`,
`Divider`, and `Card` in a runnable application:

```text
cargo run -p drawen --example custom_titlebar
```

The example also shows the full event loop: layout on resize, state updates on
maximize/fullscreen events, intent forwarding, and repainting.
