use torn_core::{Constraints, Rect};
use torn_render::{FontdueTextShaper, PaintContext, TextLayout, TextStyle};
use torn_ui::{LayoutContext, LayoutResult, UiEnvironment, Widget};

/// A widget that paints text.
///
/// [`Self::new`] accepts a pre-shaped layout for fixed-size text. Use
/// [`Self::wrapping`] for text that must reflow to its available width.
pub struct Text {
    layout: TextLayout,
    source: Option<TextSource>,
}

struct TextSource {
    text: String,
    shaper: FontdueTextShaper,
    style: TextStyle,
}

impl Text {
    /// Creates a text widget from a pre-shaped layout.
    #[must_use]
    pub const fn new(layout: TextLayout) -> Self {
        Self {
            layout,
            source: None,
        }
    }

    /// Creates text that reshapes and wraps to its available width during layout.
    #[must_use]
    pub fn wrapping(text: impl Into<String>, shaper: FontdueTextShaper, style: TextStyle) -> Self {
        let text = text.into();
        let layout = shaper.layout(&text, &style, None);
        Self {
            layout,
            source: Some(TextSource {
                text,
                shaper,
                style,
            }),
        }
    }

    /// Returns the pre-shaped text layout.
    #[must_use]
    pub const fn text_layout(&self) -> &TextLayout {
        &self.layout
    }
}

impl Widget for Text {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        if let Some(source) = &self.source {
            let max_width = constraints.max().width();
            let width = max_width.is_finite().then_some(max_width.max(0.0));
            self.layout = source.shaper.layout(&source.text, &source.style, width);
        }
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

    #[test]
    fn wraps_to_the_available_width() {
        let mut runtime = UiRuntime::new(Text::wrapping(
            "one two three",
            FontdueTextShaper::ubuntu_light(),
            TextStyle::new(16.0, Color::BLACK),
        ));

        let result = runtime
            .layout(Constraints::loose(
                torn_core::Size::new(30.0, 200.0).expect("valid text constraints"),
            ))
            .expect("wrapping text layout succeeds");

        assert!(result.size().height() > 16.0);
        assert!(result.size().width() <= 30.0);
    }
}
