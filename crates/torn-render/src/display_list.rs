use torn_core::{Color, Point, Rect};

use crate::TextLayout;

/// An ordered, backend-neutral sequence of paint operations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    commands: Vec<DisplayCommand>,
}

impl DisplayList {
    /// Creates an empty display list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Returns the paint commands in recording order.
    #[must_use]
    pub fn commands(&self) -> &[DisplayCommand] {
        &self.commands
    }

    /// Returns the number of recorded commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns whether no commands have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub(crate) fn push(&mut self, command: DisplayCommand) {
        self.commands.push(command);
    }
}

/// One backend-neutral paint operation.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCommand {
    /// Paints a filled axis-aligned rectangle using source-over composition.
    FillRect {
        /// Bounds of the rectangle in logical pixels.
        rect: Rect,
        /// Unpremultiplied sRGBA source color.
        color: Color,
    },
    /// Restricts all subsequent paint operations to the supplied rectangle.
    PushClip {
        /// Bounds of the clip in logical pixels.
        rect: Rect,
    },
    /// Removes the most recently pushed clip.
    PopClip,
    /// Draws pre-laid-out text at the supplied top-left origin.
    DrawText {
        /// Pre-laid-out text data.
        layout: TextLayout,
        /// Top-left text origin in logical pixels.
        origin: Point,
    },
}
