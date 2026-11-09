use torn_core::{Constraints, Rect};
use torn_render::{FontdueTextShaper, PaintContext, TextLayout, TextShaper, TextStyle};
use torn_ui::{LayoutContext, LayoutResult, StyleTarget, TssStyle, UiEnvironment, Widget};

/// A widget that paints text.
///
/// [`Self::new`] accepts a pre-shaped layout for fixed-size text. Use
/// [`Self::wrapping`] for text that must reflow to its available width with a
/// specific shaper, or [`Self::styled`] to use the runtime's font database.
pub struct Text {
    layout: TextLayout,
    source: Option<TextSource>,
}

struct TextSource {
    text: String,
    shaper: Option<FontdueTextShaper>,
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
    ///
    /// The supplied shaper is used for this widget. Use [`Self::styled`] when
    /// the runtime environment's font database should be used instead.
    #[must_use]
    pub fn wrapping(text: impl Into<String>, shaper: FontdueTextShaper, style: TextStyle) -> Self {
        let text = text.into();
        let layout = shaper.layout(&text, &style, None);
        Self {
            layout,
            source: Some(TextSource {
                text,
                shaper: Some(shaper),
                style,
            }),
        }
    }

    /// Creates text that reshapes and wraps with the runtime's text shaper.
    ///
    /// The style supplies the base font request and color. Per-node TSS font
    /// properties and the application default font are applied on top during
    /// layout, matching the behavior of standard controls.
    #[must_use]
    pub fn styled(text: impl Into<String>, style: TextStyle) -> Self {
        let text = text.into();
        let layout = TextLayout::empty(&text, style.color());
        Self {
            layout,
            source: Some(TextSource {
                text,
                shaper: None,
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
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        if let Some(source) = &self.source {
            let max_width = constraints.max().width();
            let width = max_width.is_finite().then_some(max_width.max(0.0));
            let font = context.environment().font_from(source.style.font(), &style);
            let text_style =
                TextStyle::new_with_font(font, style.foreground.unwrap_or(source.style.color()));
            let shaper: &dyn TextShaper = match &source.shaper {
                Some(shaper) => shaper,
                None => context.environment().text_shaper(),
            };
            self.layout = shaper.layout(&source.text, &text_style, width);
        } else if let Some(color) = style.foreground
            && self.layout.color() != color
        {
            self.layout = self.layout.clone().with_color(color);
        }
        LayoutResult::new(constraints.constrain(self.layout.size()))
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("text")
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &TssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        _: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        let layout = match style.foreground {
            Some(color) if color != self.layout.color() => self.layout.clone().with_color(color),
            _ => self.layout.clone(),
        };
        context.draw_text(layout, bounds.origin);
    }

    fn participates_in_hit_test(&self) -> bool {
        false
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
