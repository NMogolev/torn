//! Platform-independent window, event-loop, and framebuffer contracts for Torn.

use torn_core::{InputEvent, Size};
use torn_render::DisplayList;

/// Initial properties of a native application window.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    /// Window title shown by the operating system.
    pub title: String,
    /// Initial content size in logical pixels.
    pub size: Size,
}

impl WindowOptions {
    /// Creates window options with `title` and an initial logical `size`.
    #[must_use]
    pub fn new(title: impl Into<String>, size: Size) -> Self {
        Self {
            title: title.into(),
            size,
        }
    }
}

/// A native-window event translated into Torn types.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// The content area changed to this logical size.
    Resized(Size),
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
