//! Platform-independent window, event-loop, and framebuffer contracts for Torn.

use torn_core::{InputEvent, Size};
use torn_render::DisplayList;

/// Initial properties of a native application window.
// Four independent window states are clearer as named booleans than as an
// artificial state machine; every builder changes exactly one of them.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    /// Window title shown by the operating system.
    pub title: String,
    /// Initial content size in logical pixels.
    pub size: Size,
    /// Whether the operating system should draw its native window frame.
    ///
    /// Applications that provide a custom title bar set this to `false` and
    /// draw their own chrome. The winit adapter keeps window moving, resizing,
    /// minimizing, maximizing, and fullscreen available in both modes.
    pub decorations: bool,
    /// Whether the user may resize the window.
    pub resizable: bool,
    /// Whether the window starts maximized.
    pub maximized: bool,
    /// Whether the window starts in borderless fullscreen mode.
    pub fullscreen: bool,
    /// Optional minimum content size in logical pixels.
    pub min_size: Option<Size>,
}

impl WindowOptions {
    /// Creates window options with `title`, an initial logical `size`, and
    /// native decorations enabled.
    #[must_use]
    pub fn new(title: impl Into<String>, size: Size) -> Self {
        Self {
            title: title.into(),
            size,
            decorations: true,
            resizable: true,
            maximized: false,
            fullscreen: false,
            min_size: None,
        }
    }

    /// Returns these options with native decorations enabled or disabled.
    #[must_use]
    pub fn with_decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Returns these options with user resizing enabled or disabled.
    #[must_use]
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Returns these options with the initial maximized state set.
    #[must_use]
    pub fn with_maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    /// Returns these options with the initial fullscreen state set.
    #[must_use]
    pub fn with_fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }

    /// Returns these options with a minimum logical content size.
    #[must_use]
    pub fn with_min_size(mut self, min_size: Size) -> Self {
        self.min_size = Some(min_size);
        self
    }
}

/// A native-window event translated into Torn types.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// The content area changed to this logical size.
    Resized(Size),
    /// The window entered or left the maximized state.
    Maximized(bool),
    /// The window entered or left fullscreen mode.
    Fullscreen(bool),
    /// An input event relative to the content area in logical pixels.
    Input(InputEvent),
    /// The operating system requested a new frame.
    RedrawRequested,
    /// The user requested that the window be closed.
    CloseRequested,
}

/// The application's requested action after it receives a [`WindowEvent`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WindowAction {
    /// Keep processing native events without scheduling a frame.
    #[default]
    None,
    /// Ask the native window to issue a redraw event.
    RequestRedraw,
    /// Leave the native event loop.
    Exit,
    /// Minimize the native window.
    Minimize,
    /// Toggle between maximized and restored window geometry.
    ToggleMaximize,
    /// Toggle borderless fullscreen mode.
    ToggleFullscreen,
    /// Begin moving the window while the primary pointer button is held.
    StartDrag,
    /// Begin resizing the window from `direction` while the primary pointer
    /// button is held.
    StartResize(WindowResizeDirection),
}

/// An edge or corner from which a native window resize can start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowResizeDirection {
    /// Top edge.
    North,
    /// Bottom edge.
    South,
    /// Left edge.
    West,
    /// Right edge.
    East,
    /// Top-left corner.
    NorthWest,
    /// Top-right corner.
    NorthEast,
    /// Bottom-left corner.
    SouthWest,
    /// Bottom-right corner.
    SouthEast,
}

/// Application callbacks driven by a native window adapter.
pub trait WindowApplication {
    /// Returns the settings used to create the native window.
    fn window_options(&self) -> WindowOptions;

    /// Handles a translated native event.
    fn window_event(&mut self, event: WindowEvent) -> WindowAction;

    /// Records the display list for the next rendered frame.
    ///
    /// Platform adapters may execute this list asynchronously and continue to
    /// present the last completed frame while it is rendering.
    fn redraw(&mut self) -> DisplayList;
}

#[cfg(test)]
mod tests {
    use torn_core::Size;

    use super::WindowOptions;

    #[test]
    fn new_options_enable_native_decorations() {
        let options = WindowOptions::new("Torn", Size::new(640.0, 480.0).expect("valid test size"));

        assert!(options.decorations);
        assert!(options.resizable);
        assert!(!options.maximized);
        assert!(!options.fullscreen);
        assert_eq!(options.min_size, None);
    }

    #[test]
    fn builders_configure_custom_chrome() {
        let minimum = Size::new(320.0, 240.0).expect("valid test size");
        let options = WindowOptions::new("Torn", Size::new(640.0, 480.0).expect("valid test size"))
            .with_decorations(false)
            .with_resizable(false)
            .with_maximized(true)
            .with_fullscreen(true)
            .with_min_size(minimum);

        assert!(!options.decorations);
        assert!(!options.resizable);
        assert!(options.maximized);
        assert!(options.fullscreen);
        assert_eq!(options.min_size, Some(minimum));
    }
}
