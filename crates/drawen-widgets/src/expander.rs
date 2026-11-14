use drawen_core::{
    Constraints, InputEvent, Key, NamedKey, Point, PointerButton, PointerId, Rect, Size,
};
use drawen_render::{PaintContext, TextLayout, TextStyle};
use drawen_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, Border, ChildLayout, DssStyle, EventContext, EventPhase, EventStatus,
    LayoutContext, LayoutResult, PseudoClass, Signal, StyleTarget, UiEnvironment, Widget,
};

const MIN_HEADER_HEIGHT: f32 = 30.0;
const HEADER_VERTICAL_PADDING: f32 = 16.0;
const TITLE_GAP: f32 = 4.0;
const TRAILING_PADDING: f32 = 12.0;

/// A collapsible section with a clickable header and one optional body child.
///
/// The expander keeps its body child retained while collapsed but excludes it
/// from layout, painting, hit testing, and focus traversal. Pointer clicks and
/// Space or Enter atomically toggle the section and emit [`Self::toggled`].
pub struct Expander {
    title: String,
    expanded: bool,
    toggled: Signal<bool>,
    pressed_pointer: Option<PointerId>,
    header_pressed: bool,
    header_height: f32,
    size: Size,
    title_layout: Option<TextLayout>,
}

impl Expander {
    /// Creates a collapsed expander with `title`.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            expanded: false,
            toggled: Signal::new(),
            pressed_pointer: None,
            header_pressed: false,
            header_height: MIN_HEADER_HEIGHT,
            size: Size::ZERO,
            title_layout: None,
        }
    }

    /// Returns this expander with an initial expansion state.
    #[must_use]
    pub const fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Returns the header title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns whether the section is expanded.
    #[must_use]
    pub const fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Sets the expansion state without emitting [`Self::toggled`].
    ///
    /// The caller is responsible for requesting a layout pass after the change.
    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
    }

    /// Returns a signal emitted when user interaction changes the expansion state.
    #[must_use]
    pub fn toggled(&self) -> Signal<bool> {
        self.toggled.clone()
    }

    fn header_bounds(&self) -> Rect {
        Rect::new(
            Point::ZERO,
            size(
                self.size.width(),
                self.header_height.min(self.size.height()),
            ),
        )
    }

    fn toggle(&mut self, context: &mut EventContext<'_>) {
        self.expanded = !self.expanded;
        self.toggled.emit(&self.expanded);
        context.request_layout();
    }

    fn set_expanded_without_context(&mut self, expanded: bool) -> bool {
        if self.expanded == expanded {
            return false;
        }
        self.expanded = expanded;
        self.toggled.emit(&expanded);
        true
    }
}

impl Widget for Expander {
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
        let text_style = TextStyle::new_with_font(font, style.foreground_or(theme.foreground()));
        let max = constraints.max();
        let provisional_header = (font_size + HEADER_VERTICAL_PADDING).max(MIN_HEADER_HEIGHT);
        let wrap_width = max
            .width()
            .is_finite()
            .then(|| (max.width() - provisional_header - TRAILING_PADDING).max(0.0));
        let title_layout = environment
            .text_shaper()
            .layout(&self.title, &text_style, wrap_width);
        self.header_height = (title_layout.size().height().max(font_size)
            + HEADER_VERTICAL_PADDING)
            .max(MIN_HEADER_HEIGHT);
        let width = if max.width().is_finite() {
            max.width()
        } else {
            title_layout.size().width() + self.header_height + TRAILING_PADDING
        };
        self.title_layout = Some(title_layout);

        let mut children = Vec::new();
        let mut body_height = 0.0;
        if context.child_count() == 1 {
            let remaining = if max.height().is_finite() {
                (max.height() - self.header_height).max(0.0)
            } else {
                f32::INFINITY
            };
            let body_constraints = if self.expanded {
                Constraints::loose(size(width, remaining))
            } else {
                Constraints::tight(Size::ZERO).expect("zero constraints are valid")
            };
            let (id, layout) = context
                .layout_child(0, body_constraints)
                .expect("runtime child index is valid");
            if self.expanded {
                body_height = layout.size().height();
            }
            children.push(ChildLayout::with_visibility(
                id,
                Point::new(0.0, self.header_height),
                self.expanded,
            ));
        }

        self.size = constraints.constrain(size(width, self.header_height + body_height));
        LayoutResult::with_children(self.size, children)
    }

    fn style_target(&self) -> StyleTarget {
        let target = StyleTarget::new("expander");
        if self.header_pressed {
            target.with_state(PseudoClass::Pressed)
        } else {
            target
        }
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &DssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &DssStyle,
    ) {
        let theme = environment.theme();
        let radius = style.corner_radius_or(theme.corner_radius());
        let header = Rect::new(
            bounds.origin,
            size(
                bounds.size.width(),
                self.header_height.min(bounds.size.height()),
            ),
        );
        let background = style.background.unwrap_or_else(|| theme.raised_surface());
        context.fill_rounded_rect(header, radius, background);
        let border = style
            .border
            .unwrap_or_else(|| Border::new(1.0, theme.border()));
        context.stroke_rounded_rect(header, radius, border.width, border.color);

        let icon_color = style.foreground_or(theme.foreground());
        let icon_size = (self.header_height * 0.36).clamp(6.0, 14.0);
        let bar_thickness = (icon_size * 0.16).max(1.5);
        let center = Point::new(
            header.origin.x + self.header_height / 2.0,
            header.origin.y + self.header_height / 2.0,
        );
        context.fill_rounded_rect(
            Rect::new(
                Point::new(center.x - icon_size / 2.0, center.y - bar_thickness / 2.0),
                size(icon_size, bar_thickness),
            ),
            bar_thickness / 2.0,
            icon_color,
        );
        if !self.expanded {
            context.fill_rounded_rect(
                Rect::new(
                    Point::new(center.x - bar_thickness / 2.0, center.y - icon_size / 2.0),
                    size(bar_thickness, icon_size),
                ),
                bar_thickness / 2.0,
                icon_color,
            );
        }

        if let Some(layout) = &self.title_layout {
            let text_size = layout.size();
            context.draw_text(
                layout.clone(),
                Point::new(
                    header.origin.x + self.header_height + TITLE_GAP,
                    header.origin.y + (self.header_height - text_size.height()) / 2.0,
                ),
            );
        }

        if self.expanded && bounds.size.height() > self.header_height {
            let divider = theme
                .muted_foreground()
                .with_alpha(theme.muted_foreground().alpha * 0.25);
            context.fill_rect(
                Rect::new(
                    Point::new(
                        bounds.origin.x + header.size.width() * 0.02,
                        bounds.origin.y + self.header_height - 1.0,
                    ),
                    size(header.size.width() * 0.96, 1.0),
                ),
                divider,
            );
        }
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer)
                if pointer.button == Some(PointerButton::Primary)
                    && self.header_bounds().contains(pointer.position) =>
            {
                self.pressed_pointer = Some(pointer.pointer_id);
                self.header_pressed = true;
                context.capture_pointer(pointer.pointer_id);
                context.request_focus();
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer)
                if pointer.button == Some(PointerButton::Primary)
                    && self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                let activate =
                    self.header_pressed && self.header_bounds().contains(pointer.position);
                self.pressed_pointer = None;
                self.header_pressed = false;
                context.release_pointer(pointer.pointer_id);
                context.request_redraw();
                if activate {
                    self.toggle(context);
                }
                EventStatus::Handled
            }
            InputEvent::PointerCancel(pointer)
                if self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                self.pressed_pointer = None;
                self.header_pressed = false;
                context.release_pointer(pointer.pointer_id);
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::KeyDown(key)
                if !key.repeat
                    && matches!(key.key, Key::Named(NamedKey::Space | NamedKey::Enter)) =>
            {
                self.toggle(context);
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let mut accessibility = Accessibility::new(AccessibilityRole::Button)
            .with_label(self.title.clone())
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(AccessibilityActions::FOCUS | AccessibilityActions::CLICK);
        if self.expanded {
            accessibility = accessibility
                .with_states(AccessibilityState::EXPANDED)
                .with_actions(AccessibilityActions::COLLAPSE);
        } else {
            accessibility = accessibility.with_actions(AccessibilityActions::EXPAND);
        }
        accessibility
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        match action {
            AccessibilityAction::Click => self.set_expanded_without_context(!self.expanded),
            AccessibilityAction::Expand => self.set_expanded_without_context(true),
            AccessibilityAction::Collapse => self.set_expanded_without_context(false),
            _ => false,
        }
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("expander sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use drawen_core::{Modifiers, PointerButtons, PointerEvent, PointerId, Size};
    use drawen_ui::UiRuntime;

    use crate::SizedBox;

    use super::*;

    #[test]
    fn collapses_the_body_until_the_header_is_clicked() {
        let section = Expander::new("Advanced");
        let values = Rc::new(RefCell::new(Vec::new()));
        let _subscription = section.toggled().subscribe({
            let values = Rc::clone(&values);
            move |expanded| values.borrow_mut().push(*expanded)
        });
        let mut runtime = UiRuntime::new(section);
        runtime
            .append_child(runtime.root(), SizedBox::new(Some(60.0), Some(20.0)))
            .expect("expander root exists");
        let constraints = Constraints::loose(Size::new(120.0, 200.0).expect("valid constraints"));
        let collapsed = runtime
            .layout(constraints)
            .expect("collapsed expander layout succeeds")
            .size();

        let _ = runtime.dispatch_event(&pointer(true, Point::new(10.0, 5.0)));
        let _ = runtime.dispatch_event(&pointer(false, Point::new(10.0, 5.0)));
        let expanded = runtime
            .layout(constraints)
            .expect("expanded expander layout succeeds")
            .size();

        assert_eq!(*values.borrow(), vec![true]);
        assert!(expanded.height() > collapsed.height());
        assert!((expanded.height() - collapsed.height() - 20.0).abs() <= f32::EPSILON);
    }

    fn pointer(down: bool, position: Point) -> InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position,
            button: Some(PointerButton::Primary),
            buttons: if down {
                PointerButtons::PRIMARY
            } else {
                PointerButtons::NONE
            },
            modifiers: Modifiers::NONE,
        };
        if down {
            InputEvent::PointerDown(event)
        } else {
            InputEvent::PointerUp(event)
        }
    }
}
