use torn_core::{
    Color, Constraints, InputEvent, Insets, Key, NamedKey, Point, PointerButton, Rect, Size,
};
use torn_render::{PaintContext, TextLayout, TextStyle};
use torn_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    StyleTarget, TssStyle, UiEnvironment, Widget,
};

const HORIZONTAL_PADDING: f32 = 12.0;
const MIN_HEIGHT: f32 = 32.0;
const MIN_WIDTH: f32 = 120.0;

/// A horizontal set of mutually exclusive labeled options.
///
/// The control paints equal-width segments and reports the newly selected index
/// through [`Self::changed`]. Pointer clicks, arrow keys, Home, and End all
/// participate in selection.
pub struct SegmentedControl {
    labels: Vec<String>,
    layouts: Vec<TextLayout>,
    selected: Option<usize>,
    changed: Signal<usize>,
    segment_bounds: Vec<Rect>,
    size: Size,
    height: f32,
}

impl SegmentedControl {
    /// Creates a segmented control from labels, selecting the first one.
    #[must_use]
    pub fn new<T>(labels: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<String>,
    {
        let labels = labels.into_iter().map(Into::into).collect::<Vec<_>>();
        Self {
            selected: (!labels.is_empty()).then_some(0),
            labels,
            layouts: Vec::new(),
            changed: Signal::new(),
            segment_bounds: Vec::new(),
            size: Size::ZERO,
            height: MIN_HEIGHT,
        }
    }

    /// Returns the labels in display order.
    #[must_use]
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// Returns the selected index, if any.
    #[must_use]
    pub const fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Returns this control with an initial selection.
    ///
    /// # Panics
    ///
    /// Panics when `selected` does not refer to an existing label.
    #[must_use]
    pub fn with_selected(mut self, selected: usize) -> Self {
        self.set_selected(Some(selected));
        self
    }

    /// Returns this control with an explicit segment height.
    #[must_use]
    pub const fn with_height(mut self, height: f32) -> Self {
        self.height = height.max(MIN_HEIGHT);
        self
    }

    /// Returns a signal emitted when the user selects a different segment.
    #[must_use]
    pub fn changed(&self) -> Signal<usize> {
        self.changed.clone()
    }

    /// Selects a segment without emitting [`Self::changed`].
    ///
    /// # Panics
    ///
    /// Panics when `selected` does not refer to an existing label.
    pub fn set_selected(&mut self, selected: Option<usize>) {
        assert!(
            selected.is_none_or(|index| index < self.labels.len()),
            "selected segment index must refer to an existing label"
        );
        self.selected = selected;
    }

    fn select_user(&mut self, index: usize, context: &mut EventContext<'_>) {
        if self.selected == Some(index) {
            return;
        }
        self.selected = Some(index);
        self.changed.emit(&index);
        context.request_redraw();
    }

    fn select_relative(&mut self, offset: isize, context: &mut EventContext<'_>) {
        let Some(last) = self.labels.len().checked_sub(1) else {
            return;
        };
        let selected = self
            .selected
            .unwrap_or(if offset.is_negative() { last } else { 0 });
        let next = selected.saturating_add_signed(offset).min(last);
        self.select_user(next, context);
    }

    fn select_without_context(&mut self, index: usize) -> bool {
        if self.selected == Some(index) {
            return false;
        }
        self.selected = Some(index);
        self.changed.emit(&index);
        true
    }

    fn accept_accessibility_value(&mut self, value: &str) -> bool {
        let Some(index) = self.labels.iter().position(|label| label == value) else {
            return false;
        };
        self.select_without_context(index)
    }

    fn accept_accessibility_step(&mut self, offset: isize) -> bool {
        let Some(last) = self.labels.len().checked_sub(1) else {
            return false;
        };
        let selected = self
            .selected
            .unwrap_or(if offset.is_negative() { last } else { 0 });
        let next = selected.saturating_add_signed(offset).min(last);
        self.select_without_context(next)
    }

    fn index_at(&self, position: Point) -> Option<usize> {
        self.segment_bounds
            .iter()
            .position(|bounds| bounds.contains(position))
    }
}

impl Default for SegmentedControl {
    fn default() -> Self {
        Self::new(std::iter::empty::<String>())
    }
}

impl Widget for SegmentedControl {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let environment = context.environment();
        let theme = environment.theme();
        let font = environment.font_for(&style);
        let font_size = font.pixel_size(environment.logical_dpi());
        let foreground = style.foreground_or(theme.foreground());
        let text_style = TextStyle::new_with_font(font, foreground);
        let intrinsic_width = self.labels.iter().fold(MIN_WIDTH, |width, label| {
            let layout = environment.text_shaper().layout(label, &text_style, None);
            width.max(layout.size().width() + HORIZONTAL_PADDING * 2.0)
        });
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            intrinsic_width
        };
        let height = (font_size + 20.0).max(self.height);
        self.size = constraints.constrain(size(width, height));

        let count = self.labels.len();
        let segment_width = if count == 0 {
            0.0
        } else {
            self.size.width() / usize_to_f32(count)
        };
        self.segment_bounds = (0..count)
            .map(|index| {
                Rect::new(
                    Point::new(segment_width * usize_to_f32(index), 0.0),
                    size(segment_width, self.size.height()),
                )
            })
            .collect();
        self.layouts = self
            .labels
            .iter()
            .map(|label| {
                environment.text_shaper().layout(
                    label,
                    &text_style,
                    Some((segment_width - HORIZONTAL_PADDING * 2.0).max(0.0)),
                )
            })
            .collect();
        LayoutResult::new(self.size)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("segmented-control")
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &TssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        let theme = environment.theme();
        let radius = style.corner_radius_or(theme.corner_radius());
        context.fill_rounded_rect(
            bounds,
            radius,
            style.background_or(theme.control_background()),
        );
        if let Some(border) = style.border {
            context.stroke_rounded_rect(bounds, radius, border.width, border.color);
        }

        let accent = style.accent_or(theme.accent());
        let foreground = style.foreground_or(theme.foreground());
        let selected_text = contrast_color(accent);
        context.with_clip(bounds, |context| {
            for (index, segment) in self.segment_bounds.iter().enumerate() {
                let segment = offset_rect(*segment, bounds.origin);
                if self.selected == Some(index) {
                    let pill = segment.inset(Insets::all(2.0));
                    context.fill_rounded_rect(pill, (pill.size.height() / 2.0).max(0.0), accent);
                }
                let Some(layout) = self.layouts.get(index) else {
                    continue;
                };
                let color = if self.selected == Some(index) {
                    selected_text
                } else {
                    foreground
                };
                let layout = layout.clone().with_color(color);
                let text_size = layout.size();
                context.draw_text(
                    layout,
                    Point::new(
                        segment.origin.x + (segment.size.width() - text_size.width()) / 2.0,
                        segment.origin.y + (segment.size.height() - text_size.height()) / 2.0,
                    ),
                );
            }
        });
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                context.request_focus();
                if let Some(index) = self.index_at(pointer.position) {
                    self.select_user(index, context);
                }
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) if !key.repeat => match key.key {
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) => {
                    self.select_relative(-1, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::ArrowRight | NamedKey::ArrowDown) => {
                    self.select_relative(1, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::Home) if !self.labels.is_empty() => {
                    self.select_user(0, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::End) if !self.labels.is_empty() => {
                    self.select_user(self.labels.len() - 1, context);
                    EventStatus::Handled
                }
                _ => EventStatus::Ignored,
            },
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let mut accessibility = Accessibility::new(AccessibilityRole::TabList)
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(
                AccessibilityActions::FOCUS
                    | AccessibilityActions::SET_VALUE
                    | AccessibilityActions::INCREMENT
                    | AccessibilityActions::DECREMENT,
            );
        if let Some(index) = self.selected
            && let Some(label) = self.labels.get(index)
        {
            accessibility = accessibility.with_value(label);
        }
        accessibility
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        match action {
            AccessibilityAction::SetValue(value) => self.accept_accessibility_value(value),
            AccessibilityAction::Increment => self.accept_accessibility_step(1),
            AccessibilityAction::Decrement => self.accept_accessibility_step(-1),
            _ => false,
        }
    }
}

fn contrast_color(background: Color) -> Color {
    let luminance = background.red * 0.2126 + background.green * 0.7152 + background.blue * 0.0722;
    if luminance > 0.6 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

fn offset_rect(rect: Rect, offset: Point) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + offset.x, rect.origin.y + offset.y),
        rect.size,
    )
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("segmented control sizes are non-negative and not NaN")
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
    fn selects_a_segment_on_click_and_emits_its_index() {
        let control = SegmentedControl::new(["Day", "Week", "Month"]);
        assert_eq!(control.selected(), Some(0));
        let changes = Rc::new(RefCell::new(Vec::new()));
        let _subscription = control.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |index| changes.borrow_mut().push(*index)
        });
        let mut runtime = UiRuntime::new(control);
        runtime
            .layout(Constraints::tight(size(120.0, 32.0)).expect("valid constraints"))
            .expect("segmented control layout succeeds");

        let _ = runtime.dispatch_event(&InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(100.0, 5.0),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        }));

        assert_eq!(*changes.borrow(), vec![2]);
    }

    #[test]
    fn arrow_keys_move_the_selection() {
        let control = SegmentedControl::new(["One", "Two", "Three"]).with_selected(2);
        let changes = Rc::new(RefCell::new(Vec::new()));
        let _subscription = control.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |index| changes.borrow_mut().push(*index)
        });
        let mut runtime = UiRuntime::new(control);
        runtime
            .layout(Constraints::tight(size(120.0, 32.0)).expect("valid constraints"))
            .expect("segmented control layout succeeds");
        let root = runtime.root();
        runtime.focus_widget(root).expect("root is focusable");

        let _ = runtime.dispatch_event(&InputEvent::KeyDown(torn_core::KeyEvent {
            key: Key::Named(NamedKey::ArrowLeft),
            code: torn_core::KeyCode::Unidentified,
            repeat: false,
            modifiers: Modifiers::NONE,
        }));

        assert_eq!(*changes.borrow(), vec![1]);
    }
}
