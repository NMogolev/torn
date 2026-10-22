use std::{cell::RefCell, rc::Rc};

use torn_core::{Constraints, InputEvent, Key, NamedKey, Point, PointerButton, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    UiEnvironment, Widget,
};

const CONTROL_SIZE: f32 = 18.0;
const DOT_INSET: f32 = 5.0;

/// Shared selection state for a related set of [`RadioButton`] controls.
///
/// A group can have no selection. Selecting one radio button replaces the
/// group's previous value, ensuring that at most one button in the group is
/// selected at a time.
#[derive(Clone, Debug)]
pub struct RadioGroup<T> {
    selected: Rc<RefCell<Option<T>>>,
    changed: Signal<T>,
}

impl<T> RadioGroup<T>
where
    T: Clone + Eq + 'static,
{
    /// Creates a group without an initial selection.
    #[must_use]
    pub fn new() -> Self {
        Self {
            selected: Rc::new(RefCell::new(None)),
            changed: Signal::new(),
        }
    }

    /// Returns the selected value, if any.
    #[must_use]
    pub fn selected(&self) -> Option<T> {
        self.selected.borrow().clone()
    }

    /// Selects `value` and emits [`Self::changed`] when it differs from the current value.
    pub fn set_selected(&self, value: &T) {
        if self.selected.borrow().as_ref() == Some(value) {
            return;
        }
        *self.selected.borrow_mut() = Some(value.clone());
        self.changed.emit(value);
    }

    /// Clears the selection without emitting [`Self::changed`].
    pub fn clear(&self) {
        *self.selected.borrow_mut() = None;
    }

    /// Returns a signal emitted whenever a radio button selects a new value.
    #[must_use]
    pub fn changed(&self) -> Signal<T> {
        self.changed.clone()
    }
}

impl<T> Default for RadioGroup<T>
where
    T: Clone + Eq + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// A radio button that selects one value in a shared [`RadioGroup`].
///
/// The optional direct runtime-owned child is laid out as a label to the right
/// of the circular indicator.
pub struct RadioButton<T> {
    group: RadioGroup<T>,
    value: T,
    pressed_pointer: Option<torn_core::PointerId>,
    size: Size,
}

impl<T> RadioButton<T>
where
    T: Clone + Eq + 'static,
{
    /// Creates a button that selects `value` in `group`.
    #[must_use]
    pub fn new(group: RadioGroup<T>, value: T) -> Self {
        Self {
            group,
            value,
            pressed_pointer: None,
            size: Size::ZERO,
        }
    }

    /// Returns this button's value.
    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns whether this button's value is selected in its group.
    #[must_use]
    pub fn is_selected(&self) -> bool {
        self.group.selected.borrow().as_ref() == Some(&self.value)
    }

    /// Returns the group controlled by this radio button.
    #[must_use]
    pub fn group(&self) -> RadioGroup<T> {
        self.group.clone()
    }

    fn select(&self, context: &mut EventContext<'_>) {
        self.group.set_selected(&self.value);
        context.request_redraw();
    }
}

impl<T> Widget for RadioButton<T>
where
    T: Clone + Eq + 'static,
{
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let gap = context.environment().theme().spacing();
        let mut children = Vec::new();
        let label_size = if context.child_count() == 1 {
            let max = constraints.max();
            let child_max = size((max.width() - CONTROL_SIZE - gap).max(0.0), max.height());
            let (id, layout) = context
                .layout_child(0, Constraints::loose(child_max))
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(
                id,
                Point::new(
                    CONTROL_SIZE + gap,
                    (CONTROL_SIZE.max(layout.size().height()) - layout.size().height()) / 2.0,
                ),
            ));
            layout.size()
        } else {
            Size::ZERO
        };
        let size = constraints.constrain(size(
            CONTROL_SIZE
                + if context.child_count() == 1 {
                    gap + label_size.width()
                } else {
                    0.0
                },
            CONTROL_SIZE.max(label_size.height()),
        ));
        self.size = size;
        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let indicator = indicator_bounds(bounds);
        let theme = environment.theme();
        context.fill_rounded_rect(indicator, CONTROL_SIZE / 2.0, theme.background());
        context.stroke_rounded_rect(
            indicator,
            CONTROL_SIZE / 2.0,
            1.0,
            theme.button_background(),
        );
        if self.is_selected() {
            let dot = indicator.inset(torn_core::Insets::all(DOT_INSET));
            context.fill_rounded_rect(dot, dot.size.width() / 2.0, theme.accent());
        }
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.pressed_pointer = Some(pointer.pointer_id);
                context.capture_pointer(pointer.pointer_id);
                context.request_focus();
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer)
                if pointer.button == Some(PointerButton::Primary)
                    && self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                self.pressed_pointer = None;
                context.release_pointer(pointer.pointer_id);
                if self.size.contains(pointer.position) {
                    self.select(context);
                }
                EventStatus::Handled
            }
            InputEvent::PointerCancel(pointer)
                if self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                self.pressed_pointer = None;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::KeyDown(key)
                if !key.repeat
                    && matches!(key.key, Key::Named(NamedKey::Space | NamedKey::Enter)) =>
            {
                self.select(context);
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

fn indicator_bounds(bounds: Rect) -> Rect {
    Rect::new(
        Point::new(
            bounds.origin.x,
            bounds.origin.y + (bounds.size.height() - CONTROL_SIZE).max(0.0) / 2.0,
        ),
        size(CONTROL_SIZE, CONTROL_SIZE),
    )
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("radio button sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use torn_core::{Modifiers, PointerButtons, PointerEvent, PointerId};
    use torn_ui::{Column, UiRuntime};

    use super::*;

    #[test]
    fn selecting_one_button_replaces_the_previous_selection() {
        let group = RadioGroup::new();
        let changes = Rc::new(RefCell::new(Vec::new()));
        let _subscription = group.changed().subscribe({
            let changes = Rc::clone(&changes);
            move |value| changes.borrow_mut().push(*value)
        });
        let mut runtime = UiRuntime::new(Column::new());
        let root = runtime.root();
        runtime
            .append_child(root, RadioButton::new(group.clone(), 1_u8))
            .expect("root exists");
        runtime
            .append_child(root, RadioButton::new(group.clone(), 2_u8))
            .expect("root exists");
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("radio layout succeeds");

        let _ = runtime.dispatch_event(&pointer(1.0, true));
        let _ = runtime.dispatch_event(&pointer(1.0, false));
        let _ = runtime.dispatch_event(&pointer(19.0, true));
        let _ = runtime.dispatch_event(&pointer(19.0, false));

        assert_eq!(group.selected(), Some(2));
        assert_eq!(*changes.borrow(), vec![1, 2]);
    }

    fn pointer(y: f32, down: bool) -> InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(1.0, y),
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
