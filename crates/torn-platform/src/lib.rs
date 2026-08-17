//! Platform-independent window, event-loop, and framebuffer contracts for Torn.

use torn_core::{InputEvent, Size};

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

/// A mutable RGBA framebuffer in logical pixels.
///
/// A platform adapter presents the framebuffer and may scale it for the native
/// surface's device-pixel ratio.
pub struct Frame<'a> {
    size: Size,
    pixels: &'a mut [u8],
}

impl<'a> Frame<'a> {
    /// Creates a frame over `pixels`, which must contain RGBA8 texels for `size`.
    #[must_use]
    pub fn new(size: Size, pixels: &'a mut [u8]) -> Self {
        Self { size, pixels }
    }

    /// Returns the frame extent in logical pixels.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Returns the frame's RGBA8 pixels in row-major order.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        self.pixels
    }
}

/// Application callbacks driven by a native window adapter.
pub trait WindowApplication {
    /// Returns the settings used to create the native window.
    fn window_options(&self) -> WindowOptions;

    /// Handles a translated native event.
    fn window_event(&mut self, event: WindowEvent) -> WindowAction;

    /// Fills a framebuffer after a [`WindowEvent::RedrawRequested`] event.
    fn redraw(&mut self, frame: &mut Frame<'_>);
}

#[cfg(test)]
mod tests {
    use torn_core::Size;

    use super::{Frame, WindowOptions};

    #[test]
    fn frame_exposes_its_logical_extent_and_pixels() {
        let size = Size::new(2.0, 1.0).expect("valid size");
        let mut pixels = [0_u8; 8];
        let mut frame = Frame::new(size, &mut pixels);
        frame.pixels_mut()[3] = 255;

        assert_eq!(frame.size(), size);
        assert_eq!(pixels[3], 255);
        assert_eq!(WindowOptions::new("Torn", size).title, "Torn");
    }
}
