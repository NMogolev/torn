use drawen_core::{Constraints, InputEvent, Rect};
use drawen_render::PaintContext;

use crate::{
    DssStyle, EventContext, EventStatus, LayoutContext, LayoutResult, StyleTarget, UiEnvironment,
    Widget,
};

/// A transparent widget wrapper that adds DSS ids and classes.
///
/// The wrapper preserves the inner widget's type name and behavior, so a
/// `Styled<Button>` still matches `button` selectors while also matching
/// `#save` and `.primary` rules:
///
/// ```ignore
/// let save = Styled::new(Button::new())
///     .with_id("save")
///     .with_class("primary");
/// ```
pub struct Styled<W> {
    inner: W,
    id: Option<String>,
    classes: Vec<String>,
}

impl<W> Styled<W> {
    /// Wraps `inner` without additional style metadata.
    #[must_use]
    pub const fn new(inner: W) -> Self {
        Self {
            inner,
            id: None,
            classes: Vec::new(),
        }
    }

    /// Assigns the id matched by `#id` selectors.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Adds a class matched by `.class` selectors.
    #[must_use]
    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        let class = class.into();
        if !self.classes.contains(&class) {
            self.classes.push(class);
        }
        self
    }

    /// Returns the wrapped widget.
    #[must_use]
    pub const fn inner(&self) -> &W {
        &self.inner
    }

    /// Returns mutable access to the wrapped widget.
    pub const fn inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Returns the wrapped widget.
    #[must_use]
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W> Default for Styled<W>
where
    W: Default,
{
    fn default() -> Self {
        Self::new(W::default())
    }
}

impl<W> Widget for Styled<W>
where
    W: Widget,
{
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        self.inner.layout(context, constraints)
    }

    fn style_target(&self) -> StyleTarget {
        let mut target = self.inner.style_target();
        if let Some(id) = &self.id {
            target = target.with_id(id.clone());
        }
        for class in &self.classes {
            target = target.with_class(class.clone());
        }
        target
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.inner.paint(context, environment, bounds);
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &DssStyle,
    ) {
        self.inner.paint_styled(context, environment, bounds, style);
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        self.inner.handle_event(context, event)
    }

    fn accepts_focus(&self) -> bool {
        self.inner.accepts_focus()
    }

    fn participates_in_hit_test(&self) -> bool {
        self.inner.participates_in_hit_test()
    }

    fn clips_children(&self) -> bool {
        self.inner.clips_children()
    }
}
