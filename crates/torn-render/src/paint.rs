use torn_core::{Color, Point, Rect};

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

    /// Starts a rectangular clip scope.
    pub fn push_clip(&mut self, rect: Rect) {
        self.display_list.push(DisplayCommand::PushClip { rect });
    }

    /// Ends the most recently started clip scope.
    pub fn pop_clip(&mut self) {
        self.display_list.push(DisplayCommand::PopClip);
    }

    /// Records a draw operation for already shaped and measured text.
    pub fn draw_text(&mut self, layout: TextLayout, origin: Point) {
        self.display_list
            .push(DisplayCommand::DrawText { layout, origin });
    }
}

#[cfg(test)]
mod tests {
    use torn_core::{Color, Point, Rect, Size};

    use crate::{DisplayCommand, DisplayList, PaintContext};

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn records_paint_operations_in_order() {
        let rect = Rect::new(Point::new(1.0, 2.0), size(3.0, 4.0));
        let mut display_list = DisplayList::new();
        let mut paint = PaintContext::new(&mut display_list);

        paint.push_clip(rect);
        paint.fill_rect(rect, Color::BLACK);
        paint.pop_clip();

        assert_eq!(
            display_list.commands(),
            &[
                DisplayCommand::PushClip { rect },
                DisplayCommand::FillRect {
                    rect,
                    color: Color::BLACK,
                },
                DisplayCommand::PopClip,
            ]
        );
    }
}
