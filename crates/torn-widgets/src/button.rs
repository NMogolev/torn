use std::boxed::Box as HeapBox;

use torn_core::{Color, Constraints, InputEvent, Insets, Point, PointerButton, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{ChildLayout, EventStatus, LayoutResult, Widget};

/// A clickable single-child control with a rectangular background.
pub struct Button {
    background: Color,
    pressed_background: Color,
    child: HeapBox<dyn Widget>,
    on_click: Option<HeapBox<dyn FnMut()>>,
    padding: Insets,
    pressed: bool,
    last_layout: Option<LayoutResult>,
}

impl Button {
    /// Creates a button containing `child` with an 8-pixel inset on every edge.
    #[must_use]
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            background: Color::rgba8(235, 235, 235, 255),
            pressed_background: Color::rgba8(210, 210, 210, 255),
            child: HeapBox::new(child),
            on_click: None,
            padding: Insets::all(8.0),
            pressed: false,
            last_layout: None,
        }
    }

    /// Sets the button's normal and pressed background colors.
    pub fn set_backgrounds(&mut self, normal: Color, pressed: Color) {
        self.background = normal;
        self.pressed_background = pressed;
    }

    /// Sets the inset between the button edge and its child.
    pub fn set_padding(&mut self, padding: Insets) {
        self.padding = padding;
        self.last_layout = None;
    }

    /// Sets a callback invoked for a primary pointer press within the button.
    pub fn set_on_click(&mut self, callback: impl FnMut() + 'static) {
        self.on_click = Some(HeapBox::new(callback));
    }

    /// Returns whether the button is currently pressed.
    #[must_use]
    pub const fn is_pressed(&self) -> bool {
        self.pressed
    }
}

impl Widget for Button {
    fn layout(&mut self, constraints: Constraints) -> LayoutResult {
        let max = constraints.max();
        let child_max = size(
            (max.width() - self.padding.horizontal()).max(0.0),
            (max.height() - self.padding.vertical()).max(0.0),
        );
        let child_result = self.child.layout(Constraints::loose(child_max));
        let child_size = child_result.size();
        let size = size(
            child_size.width() + self.padding.horizontal(),
            child_size.height() + self.padding.vertical(),
        );
        let result = LayoutResult::with_children(
            constraints.constrain(size),
            vec![ChildLayout::new(
                Point::new(self.padding.left, self.padding.top),
                child_result,
            )],
        );
        self.last_layout = Some(result.clone());
        result
    }

    fn paint(&self, context: &mut PaintContext<'_>, origin: Point) {
        let Some(layout) = &self.last_layout else {
            return;
        };
        let background = if self.pressed {
            self.pressed_background
        } else {
            self.background
        };
        context.fill_rect(Rect::new(origin, layout.size()), background);

        if let Some(child_layout) = layout.children().first() {
            self.child.paint(
                context,
                Point::new(
                    origin.x + child_layout.origin().x,
                    origin.y + child_layout.origin().y,
                ),
            );
        }
    }

    fn handle_event(&mut self, event: &InputEvent) -> EventStatus {
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.pressed = true;
                if let Some(callback) = &mut self.on_click {
                    callback();
                }
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.pressed = false;
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("button layout sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use torn_core::{
        Color, Constraints, InputEvent, Modifiers, Point, PointerButton, PointerButtons,
        PointerEvent, PointerId, Size,
    };
    use torn_render::{DisplayCommand, DisplayList, PaintContext};
    use torn_ui::{EventStatus, LayoutResult, Widget};

    use super::Button;

    struct Fixed(Size);

    impl Widget for Fixed {
        fn layout(&mut self, constraints: Constraints) -> LayoutResult {
            LayoutResult::new(constraints.constrain(self.0))
        }
    }

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    fn pointer_event(button: PointerButton, down: bool) -> InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(1.0, 1.0),
            button: Some(button),
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

    #[test]
    fn includes_padding_and_invokes_callback_for_primary_clicks() {
        let calls = Rc::new(Cell::new(0));
        let mut button = Button::new(Fixed(size(20.0, 10.0)));
        button.set_padding(torn_core::Insets::all(4.0));
        button.set_on_click({
            let calls = Rc::clone(&calls);
            move || calls.set(calls.get() + 1)
        });

        let layout = button.layout(Constraints::UNBOUNDED);
        assert_eq!(layout.size(), size(28.0, 18.0));
        assert_eq!(layout.children()[0].origin(), Point::new(4.0, 4.0));
        assert_eq!(
            button.handle_event(&pointer_event(PointerButton::Primary, true)),
            EventStatus::Handled
        );
        assert!(button.is_pressed());
        assert_eq!(calls.get(), 1);
        assert_eq!(
            button.handle_event(&pointer_event(PointerButton::Primary, false)),
            EventStatus::Handled
        );
        assert!(!button.is_pressed());
    }

    #[test]
    fn paints_its_pressed_background() {
        let normal = Color::BLACK;
        let pressed = Color::WHITE;
        let mut button = Button::new(Fixed(size(1.0, 1.0)));
        button.set_backgrounds(normal, pressed);
        let _ = button.layout(Constraints::UNBOUNDED);
        assert_eq!(
            button.handle_event(&pointer_event(PointerButton::Primary, true)),
            EventStatus::Handled
        );
        let mut list = DisplayList::new();

        button.paint(&mut PaintContext::new(&mut list), Point::ZERO);

        assert_eq!(
            list.commands().first(),
            Some(&DisplayCommand::FillRect {
                rect: torn_core::Rect::new(Point::ZERO, size(17.0, 17.0)),
                color: pressed,
            })
        );
    }
}
