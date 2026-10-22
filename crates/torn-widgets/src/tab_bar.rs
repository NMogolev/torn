use torn_core::{Constraints, InputEvent, Point, PointerButton, Rect, Size};
use torn_render::{PaintContext, TextStyle};
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    UiEnvironment, Widget,
};

const DEFAULT_TAB_HEIGHT: f32 = 30.0;
const HORIZONTAL_PADDING: f32 = 10.0;
const VERTICAL_TEXT_OFFSET: f32 = 8.0;

/// A standard tab strip that shows exactly one runtime-owned page at a time.
///
/// Supply labels during construction and append a matching page child for each
/// label. Extra children remain retained but hidden. User selection emits the
/// active tab index through [`Self::changed`].
pub struct TabBar {
    tabs: Vec<String>,
    selected: Option<usize>,
    changed: Signal<usize>,
    tab_bounds: Vec<Rect>,
    size: Size,
    tab_height: f32,
}

impl TabBar {
    /// Creates a tab bar with its first tab active, when present.
    #[must_use]
    pub fn new<T>(tabs: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<String>,
    {
        let tabs = tabs.into_iter().map(Into::into).collect::<Vec<_>>();
        Self {
            selected: (!tabs.is_empty()).then_some(0),
            tabs,
            changed: Signal::new(),
            tab_bounds: Vec::new(),
            size: Size::ZERO,
            tab_height: DEFAULT_TAB_HEIGHT,
        }
    }

    /// Returns the tab labels in display order.
    #[must_use]
    pub fn tabs(&self) -> &[String] {
        &self.tabs
    }

    /// Returns the active tab index, if one is selected.
    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Selects a tab without emitting [`Self::changed`].
    ///
    /// # Panics
    ///
    /// Panics when `selected` refers to no tab label.
    pub fn set_selected(&mut self, selected: Option<usize>) {
        assert!(
            selected.is_none_or(|index| index < self.tabs.len()),
            "selected tab index must refer to an existing tab"
        );
        self.selected = selected;
    }

    /// Returns a signal emitted when a user activates a different tab.
    #[must_use]
    pub fn changed(&self) -> Signal<usize> {
        self.changed.clone()
    }

    fn select_user(&mut self, index: usize, context: &mut EventContext<'_>) {
        if self.selected != Some(index) {
            self.selected = Some(index);
            self.changed.emit(&index);
            context.request_layout();
        }
    }
}

impl Widget for TabBar {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            self.tabs.iter().fold(0.0, |total, tab| {
                let layout = context.environment().text_shaper().layout(
                    tab,
                    &TextStyle::new(
                        context.environment().theme().font_size(),
                        context.environment().theme().foreground(),
                    ),
                    None,
                );
                total + layout.size().width() + HORIZONTAL_PADDING * 2.0
            })
        };
        self.tab_height =
            (context.environment().theme().font_size() + 16.0).max(DEFAULT_TAB_HEIGHT);
        let max_height = if max.height().is_finite() {
            max.height()
        } else {
            self.tab_height
        };
        self.size = constraints.constrain(size(width, max_height));
        self.tab_bounds.clear();
        let tab_count = self.tabs.len();
        if tab_count > 0 {
            let tab_width = self.size.width() / usize_to_f32(tab_count);
            for index in 0..tab_count {
                self.tab_bounds.push(Rect::new(
                    Point::new(tab_width * usize_to_f32(index), 0.0),
                    size(tab_width, self.tab_height.min(self.size.height())),
                ));
            }
        }

        let page_size = size(
            self.size.width(),
            (self.size.height() - self.tab_height).max(0.0),
        );
        let mut children = Vec::with_capacity(context.child_count());
        for index in 0..context.child_count() {
            let visible = self.selected == Some(index) && index < self.tabs.len();
            let (id, _) = context
                .layout_child(
                    index,
                    Constraints::tight(if visible { page_size } else { Size::ZERO })
                        .expect("tab page size is finite"),
                )
                .expect("runtime child index is valid");
            children.push(ChildLayout::with_visibility(
                id,
                Point::new(0.0, self.tab_height),
                visible,
            ));
        }
        LayoutResult::with_children(self.size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let theme = environment.theme();
        context.fill_rect(bounds, theme.background());
        context.with_clip(bounds, |context| {
            for (index, (tab, tab_bounds)) in self.tabs.iter().zip(&self.tab_bounds).enumerate() {
                let tab_bounds = offset_rect(*tab_bounds, bounds.origin);
                let color = if self.selected == Some(index) {
                    theme.accent()
                } else {
                    theme.button_background()
                };
                context.fill_rect(tab_bounds, color);
                let text = environment.text_shaper().layout(
                    tab,
                    &TextStyle::new(theme.font_size(), theme.foreground()),
                    Some((tab_bounds.size.width() - HORIZONTAL_PADDING * 2.0).max(0.0)),
                );
                context.draw_text(
                    text,
                    Point::new(
                        tab_bounds.origin.x + HORIZONTAL_PADDING,
                        tab_bounds.origin.y + VERTICAL_TEXT_OFFSET,
                    ),
                );
            }
        });
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        let InputEvent::PointerDown(pointer) = event else {
            return EventStatus::Ignored;
        };
        if pointer.button != Some(PointerButton::Primary) {
            return EventStatus::Ignored;
        }
        let Some(index) = self
            .tab_bounds
            .iter()
            .position(|bounds| bounds.contains(pointer.position))
        else {
            return EventStatus::Ignored;
        };
        context.request_focus();
        self.select_user(index, context);
        EventStatus::Handled
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

fn offset_rect(rect: Rect, offset: Point) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + offset.x, rect.origin.y + offset.y),
        rect.size,
    )
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("tab bar sizes are non-negative and not NaN")
}

fn usize_to_f32(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{Modifiers, PointerButtons, PointerEvent, PointerId};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn selects_a_tab_and_emits_its_index() {
        let tab_bar = TabBar::new(["First", "Second"]);
        assert_eq!(tab_bar.selected(), Some(0));
        let changes = Rc::new(RefCell::new(Vec::new()));
        let _subscription = tab_bar.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |index| changes.borrow_mut().push(*index)
        });
        let mut runtime = UiRuntime::new(tab_bar);
        runtime
            .layout(Constraints::tight(size(100.0, 50.0)).expect("valid constraints"))
            .expect("tab bar layout succeeds");

        let _ = runtime.dispatch_event(&InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(75.0, 5.0),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        }));

        assert_eq!(*changes.borrow(), vec![1]);
    }
}
