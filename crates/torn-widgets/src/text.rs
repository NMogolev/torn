use torn_core::{Constraints, Point};
use torn_render::{PaintContext, TextLayout};
use torn_ui::{LayoutResult, Widget};

/// A widget that paints a pre-shaped text layout.
///
/// Text shaping remains separate from the widget layer. Construct `Text` from
/// a [`TextLayout`] supplied by a [`torn_render::TextShaper`].
pub struct Text {
    layout: TextLayout,
}

impl Text {
    /// Creates a text widget from a pre-shaped layout.
    #[must_use]
    pub const fn new(layout: TextLayout) -> Self {
        Self { layout }
    }

    /// Returns the pre-shaped text layout.
    #[must_use]
    pub const fn text_layout(&self) -> &TextLayout {
        &self.layout
    }
}

impl Widget for Text {
    fn layout(&mut self, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(constraints.constrain(self.layout.size()))
    }

    fn paint(&self, context: &mut PaintContext<'_>, origin: Point) {
        context.draw_text(self.layout.clone(), origin);
    }
}

#[cfg(test)]
mod tests {
    use torn_core::{Color, Constraints, Point, Size};
    use torn_render::{DisplayCommand, DisplayList, PaintContext, TextLayout};
    use torn_ui::Widget;

    use super::Text;

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn uses_its_text_layout_for_layout_and_paint() {
        let layout = TextLayout::new(size(30.0, 12.0), Color::BLACK);
        let mut text = Text::new(layout.clone());
        let mut list = DisplayList::new();

        assert_eq!(text.layout(Constraints::UNBOUNDED).size(), size(30.0, 12.0));
        text.paint(&mut PaintContext::new(&mut list), Point::new(5.0, 7.0));

        assert_eq!(
            list.commands(),
            &[DisplayCommand::DrawText {
                layout,
                origin: Point::new(5.0, 7.0),
            }]
        );
    }
}
