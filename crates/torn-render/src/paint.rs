use torn_core::{Affine, Color, Point, Rect};

use crate::{DisplayCommand, DisplayList, TextLayout};

/// A recording interface that appends paint operations to a [`DisplayList`].
pub struct PaintContext<'a> {
    display_list: &'a mut DisplayList,
}

impl<'a> PaintContext<'a> {
    /// Creates a context that records into `display_list`.
    pub fn new(display_list: &'a mut DisplayList) -> Self {
        Self { display_list }
    }

    /// Records a filled rectangle.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.display_list
            .push(DisplayCommand::FillRect { rect, color });
    }

    /// Records a filled rectangle with circular corners.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        self.display_list.push(DisplayCommand::FillRoundedRect {
            rect,
            radius,
            color,
        });
    }

    /// Records a rectangular border centered on the rectangle's edge.
    pub fn stroke_rect(&mut self, rect: Rect, width: f32, color: Color) {
        self.display_list
            .push(DisplayCommand::StrokeRect { rect, width, color });
    }

    /// Records a rounded rectangular border centered on the shape's edge.
    pub fn stroke_rounded_rect(&mut self, rect: Rect, radius: f32, width: f32, color: Color) {
        self.display_list.push(DisplayCommand::StrokeRoundedRect {
            rect,
            radius,
            width,
            color,
        });
    }

    /// Saves the current clip and transform state.
    pub fn save(&mut self) {
        self.display_list.push(DisplayCommand::Save);
    }

    /// Restores the most recently saved clip and transform state.
    pub fn restore(&mut self) {
        self.display_list.push(DisplayCommand::Restore);
    }

    /// Executes `paint` with an isolated clip and transform state.
    ///
    /// The state is restored when `paint` returns, preventing an unbalanced
    /// scope from leaking into subsequent drawing operations.
    pub fn with_save(&mut self, paint: impl FnOnce(&mut Self)) {
        self.save();
        paint(self);
        self.restore();
    }

    /// Concatenates `transform` with the current transform.
    pub fn transform(&mut self, transform: Affine) {
        self.display_list
            .push(DisplayCommand::Transform { transform });
    }

    /// Translates all following operations in the current state scope.
    pub fn translate(&mut self, offset: Point) {
        self.transform(Affine::translate(offset.x, offset.y));
    }

    /// Starts a rectangular clip scope.
    pub fn push_clip(&mut self, rect: Rect) {
        self.save();
        self.clip_rect(rect);
    }

    /// Ends the most recently started clip scope.
    pub fn pop_clip(&mut self) {
        self.restore();
    }

    /// Intersects the current clip with `rect` in the current transform.
    pub fn clip_rect(&mut self, rect: Rect) {
        self.display_list.push(DisplayCommand::PushClip { rect });
    }

    /// Executes `paint` clipped to `rect` without leaking state afterward.
    pub fn with_clip(&mut self, rect: Rect, paint: impl FnOnce(&mut Self)) {
        self.with_save(|context| {
            context.clip_rect(rect);
            paint(context);
        });
    }

    /// Executes `paint` under `transform` without leaking state afterward.
    pub fn with_transform(&mut self, transform: Affine, paint: impl FnOnce(&mut Self)) {
        self.with_save(|context| {
            context.transform(transform);
            paint(context);
        });
    }

    /// Records a draw operation for already shaped and measured text.
    pub fn draw_text(&mut self, layout: TextLayout, origin: Point) {
        self.display_list
            .push(DisplayCommand::DrawText { layout, origin });
    }
}

#[cfg(test)]
mod tests {
    use torn_core::{Affine, Color, Point, Rect, Size};

    use crate::{DisplayCommand, DisplayList, PaintContext};

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn records_paint_operations_in_order() {
        let rect = Rect::new(Point::new(1.0, 2.0), size(3.0, 4.0));
        let mut display_list = DisplayList::new();
        let mut paint = PaintContext::new(&mut display_list);

        paint.with_clip(rect, |context| context.fill_rect(rect, Color::BLACK));

        assert_eq!(
            display_list.commands(),
            &[
                DisplayCommand::Save,
                DisplayCommand::PushClip { rect },
                DisplayCommand::FillRect {
                    rect,
                    color: Color::BLACK,
                },
                DisplayCommand::Restore,
            ]
        );
    }

    #[test]
    fn scopes_restore_transforms_before_the_next_operation() {
        let rect = Rect::new(Point::ZERO, size(1.0, 1.0));
        let mut display_list = DisplayList::new();
        let mut paint = PaintContext::new(&mut display_list);

        paint.with_transform(Affine::translate(4.0, 5.0), |context| {
            context.fill_rect(rect, Color::BLACK);
        });
        paint.fill_rect(rect, Color::WHITE);

        assert!(matches!(
            display_list.commands(),
            [
                DisplayCommand::Save,
                DisplayCommand::Transform { .. },
                DisplayCommand::FillRect { .. },
                DisplayCommand::Restore,
                DisplayCommand::FillRect {
                    color: Color::WHITE,
                    ..
                },
            ]
        ));
    }
}
