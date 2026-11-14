use std::sync::Arc;

use drawen_core::{Affine, Color, Point, Rect};

use crate::{RasterImage, TextLayout};

/// Per-command counters for one [`DisplayList`].
///
/// Backends use these counters to understand where a frame's draw work comes
/// from before optimizing batching or caching. They are cheap to compute and do
/// not allocate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DisplayListStats {
    /// Number of [`DisplayCommand::FillRect`] operations.
    pub fills: usize,
    /// Number of [`DisplayCommand::FillRoundedRect`] operations.
    pub rounded_fills: usize,
    /// Number of [`DisplayCommand::StrokeRect`] operations.
    pub strokes: usize,
    /// Number of [`DisplayCommand::StrokeRoundedRect`] operations.
    pub rounded_strokes: usize,
    /// Number of [`DisplayCommand::Shadow`] operations.
    pub shadows: usize,
    /// Number of [`DisplayCommand::DrawText`] operations.
    pub text_runs: usize,
    /// Number of [`DisplayCommand::DrawImage`] operations.
    pub images: usize,
    /// Number of [`DisplayCommand::DrawTexture`] operations.
    pub textures: usize,
    /// Number of clipping regions pushed.
    pub clips: usize,
    /// Number of save/restore and transform state operations.
    pub state_changes: usize,
}

impl DisplayListStats {
    /// Returns the total number of recorded commands.
    #[must_use]
    pub const fn commands(self) -> usize {
        self.fills
            + self.rounded_fills
            + self.strokes
            + self.rounded_strokes
            + self.shadows
            + self.text_runs
            + self.images
            + self.textures
            + self.clips
            + self.state_changes
    }
}

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

    /// Creates an empty display list with room for `capacity` commands.
    ///
    /// Retained runtimes can pre-size their frame buffer to avoid reallocating
    /// on the first frames of a new scene.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            commands: Vec::with_capacity(capacity),
        }
    }

    /// Removes every recorded command while retaining the allocated capacity.
    ///
    /// Reuse one display list across frames instead of allocating a fresh list
    /// per frame; the recorded command count returns to zero without releasing
    /// the underlying buffer.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Reserves capacity for at least `additional` more commands.
    pub fn reserve(&mut self, additional: usize) {
        self.commands.reserve(additional);
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

    /// Returns per-command counters for profiling.
    #[must_use]
    pub fn stats(&self) -> DisplayListStats {
        let mut stats = DisplayListStats::default();
        for command in &self.commands {
            match command {
                DisplayCommand::FillRect { .. } => stats.fills += 1,
                DisplayCommand::FillRoundedRect { .. } => stats.rounded_fills += 1,
                DisplayCommand::StrokeRect { .. } => stats.strokes += 1,
                DisplayCommand::StrokeRoundedRect { .. } => stats.rounded_strokes += 1,
                DisplayCommand::Shadow { .. } => stats.shadows += 1,
                DisplayCommand::DrawText { .. } => stats.text_runs += 1,
                DisplayCommand::DrawImage { .. } => stats.images += 1,
                DisplayCommand::DrawTexture { .. } => stats.textures += 1,
                DisplayCommand::PushClip { .. } => stats.clips += 1,
                DisplayCommand::Save
                | DisplayCommand::Restore
                | DisplayCommand::Transform { .. }
                | DisplayCommand::PopClip => stats.state_changes += 1,
            }
        }
        stats
    }

    /// Returns a conservative bounding rectangle for all recorded paint.
    ///
    /// The result unions command bounds without applying transforms, so it is
    /// never smaller than the truly painted area. It is suitable for damage
    /// tracking and diagnostics, not for exact culling.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        self.commands
            .iter()
            .filter_map(command_bounds)
            .reduce(Rect::union)
    }

    /// Appends a copy of all commands from `other` in recording order.
    ///
    /// This is useful for retained runtimes that reuse an unchanged display
    /// list while preserving commands already recorded by the caller.
    pub fn extend_from(&mut self, other: &Self) {
        self.commands.extend_from_slice(&other.commands);
    }

    pub(crate) fn push(&mut self, command: DisplayCommand) {
        self.commands.push(command);
    }
}

fn command_bounds(command: &DisplayCommand) -> Option<Rect> {
    match command {
        DisplayCommand::FillRect { rect, .. }
        | DisplayCommand::FillRoundedRect { rect, .. }
        | DisplayCommand::StrokeRect { rect, .. }
        | DisplayCommand::StrokeRoundedRect { rect, .. }
        | DisplayCommand::PushClip { rect }
        | DisplayCommand::DrawImage {
            destination: rect, ..
        }
        | DisplayCommand::DrawTexture {
            destination: rect, ..
        } => Some(*rect),
        DisplayCommand::Shadow { rect, blur, .. } => Some(inflate(*rect, *blur)),
        DisplayCommand::DrawText { layout, origin } => Some(Rect::new(*origin, layout.size())),
        DisplayCommand::Save
        | DisplayCommand::Restore
        | DisplayCommand::Transform { .. }
        | DisplayCommand::PopClip => None,
    }
}

fn inflate(rect: Rect, amount: f32) -> Rect {
    Rect::new(
        Point::new(rect.origin.x - amount, rect.origin.y - amount),
        drawen_core::Size::new(
            rect.size.width() + amount * 2.0,
            rect.size.height() + amount * 2.0,
        )
        .expect("inflating a finite rectangle stays valid"),
    )
}

/// One backend-neutral paint operation.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCommand {
    /// Saves the current clip and transform state.
    Save,
    /// Restores the most recently saved clip and transform state.
    Restore,
    /// Concatenates an affine transform with the current transform.
    Transform {
        /// Transform expressed in logical-pixel coordinates.
        transform: Affine,
    },
    /// Paints a filled axis-aligned rectangle using source-over composition.
    FillRect {
        /// Bounds of the rectangle in logical pixels.
        rect: Rect,
        /// Unpremultiplied sRGBA source color.
        color: Color,
    },
    /// Paints a filled rectangle with circular corners.
    FillRoundedRect {
        /// Bounds of the rectangle in logical pixels.
        rect: Rect,
        /// Radius of every corner in logical pixels.
        radius: f32,
        /// Unpremultiplied sRGBA source color.
        color: Color,
    },
    /// Paints an inside-and-outside centered rectangular border.
    StrokeRect {
        /// Bounds of the rectangle's centerline in logical pixels.
        rect: Rect,
        /// Border width in logical pixels.
        width: f32,
        /// Unpremultiplied sRGBA source color.
        color: Color,
    },
    /// Paints an inside-and-outside centered rounded rectangular border.
    StrokeRoundedRect {
        /// Bounds of the rounded rectangle's centerline in logical pixels.
        rect: Rect,
        /// Radius of every corner on the centerline, in logical pixels.
        radius: f32,
        /// Border width in logical pixels.
        width: f32,
        /// Unpremultiplied sRGBA source color.
        color: Color,
    },
    /// Paints a soft drop shadow behind a rounded rectangle.
    ///
    /// The shape at `rect` is filled fully, then fades to transparent over
    /// `blur` logical pixels beyond its edge. Backends render this with an
    /// analytic blur rather than by compositing many stacked rectangles, so one
    /// shadow command stays cheap enough for hover states and popups.
    Shadow {
        /// Bounds of the solid part of the shadow in logical pixels.
        rect: Rect,
        /// Radius of every corner in logical pixels.
        radius: f32,
        /// Width of the soft falloff outside `rect`, in logical pixels.
        blur: f32,
        /// Unpremultiplied sRGBA shadow color.
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
        /// Pre-laid-out text data, shared with the caller so repainting an
        /// unchanged layout does not copy its glyph runs.
        layout: Arc<TextLayout>,
        /// Top-left text origin in logical pixels.
        origin: Point,
    },
    /// Draws a raster image scaled into the supplied destination rectangle.
    DrawImage {
        /// Source pixels in 8-bit sRGBA format, shared with the caller so
        /// repainting an unchanged image does not copy its pixel buffer.
        image: Arc<RasterImage>,
        /// Destination rectangle in logical pixels.
        destination: Rect,
    },
    /// Draws a GPU texture registered with the backend out of band.
    ///
    /// The software renderer cannot resolve these textures; it reports an
    /// unsupported-operation error instead.
    DrawTexture {
        /// Identifier previously registered with the GPU renderer.
        texture: TextureId,
        /// Destination rectangle in logical pixels.
        destination: Rect,
    },
}

/// Identifies a GPU texture supplied to a renderer outside the display list.
///
/// Drawen's browser layer allocates a shared Servo texture and registers it with
/// the GPU renderer under one of these identifiers. Display lists then refer to
/// the texture without owning it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureId(u64);

impl TextureId {
    /// Wraps a raw identifier supplied by a renderer or browser layer.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use drawen_core::{Color, Point, Rect, Size};

    use super::{DisplayCommand, DisplayList};

    fn rect() -> Rect {
        Rect::new(Point::ZERO, Size::new(1.0, 1.0).expect("valid test size"))
    }

    #[test]
    fn clear_retains_the_command_buffer_for_reuse() {
        let mut list = DisplayList::with_capacity(16);
        assert!(list.commands.capacity() >= 16);
        list.push(DisplayCommand::FillRect {
            rect: rect(),
            color: Color::BLACK,
        });
        let capacity = list.commands.capacity();

        list.clear();

        assert!(list.is_empty());
        assert_eq!(list.commands.capacity(), capacity);
        list.push(DisplayCommand::FillRect {
            rect: rect(),
            color: Color::WHITE,
        });
        assert_eq!(list.len(), 1);
        assert_eq!(list.commands.capacity(), capacity);
    }
}
