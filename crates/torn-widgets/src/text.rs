use torn_core::{Constraints, Rect};
use torn_render::{PaintContext, TextLayout};
use torn_ui::{LayoutContext, LayoutResult, UiEnvironment, Widget};

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
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(constraints.constrain(self.layout.size()))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.draw_text(self.layout.clone(), bounds.origin);
    }
}

#[cfg(test)]
mod tests {
    use torn_core::{Color, Constraints, Point};
    use torn_render::{DisplayCommand, DisplayList, FontdueTextShaper, PaintContext, TextStyle};
    use torn_ui::UiRuntime;

    use super::Text;

    #[test]
    fn uses_its_text_layout_for_layout_and_paint() {
        let layout = FontdueTextShaper::ubuntu_light().layout(
            "Text",
            &TextStyle::new(12.0, Color::BLACK),
            None,
        );
        let mut runtime = UiRuntime::new(Text::new(layout.clone()));
        let mut list = DisplayList::new();

        assert_eq!(
            runtime
                .layout(Constraints::UNBOUNDED)
                .expect("text layout succeeds")
                .size(),
            layout.size()
        );
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("text paint succeeds");

        assert_eq!(
            list.commands(),
            &[DisplayCommand::DrawText {
                layout,
                origin: Point::ZERO,
            }]
        );
    }
}
