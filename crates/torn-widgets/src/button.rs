use torn_core::{
    Color, Constraints, InputEvent, Insets, Key, NamedKey, Point, PointerButton, Rect, Size,
};
use torn_render::PaintContext;
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal,
    UiEnvironment, Widget,
};

/// A clickable single-child control with a rectangular background.
///
/// Its direct child is owned by [`torn_ui::UiRuntime`] and should be appended to
/// the button node before layout.
pub struct Button {
    backgrounds: Option<(Color, Color)>,
    activated: Signal<()>,
    padding: Insets,
    pressed: bool,
    pressed_pointer: Option<torn_core::PointerId>,
    pointer_inside: bool,
    size: Size,
}

impl Button {
    /// Creates an empty button with an 8-pixel inset on every edge.
    ///
    /// Its colors are resolved from the runtime [`torn_ui::Theme`] until
    /// [`Self::set_backgrounds`] is called.
    #[must_use]
    pub fn new() -> Self {
        Self {
            backgrounds: None,
            activated: Signal::new(),
            padding: Insets::all(8.0),
            pressed: false,
            pressed_pointer: None,
            pointer_inside: false,
            size: Size::ZERO,
        }
    }

    /// Sets the button's normal and pressed background colors.
    pub fn set_backgrounds(&mut self, normal: Color, pressed: Color) {
        self.backgrounds = Some((normal, pressed));
    }

    /// Sets the inset between the button edge and its child.
    pub fn set_padding(&mut self, padding: Insets) {
        self.padding = padding;
    }

    /// Returns a signal emitted after a completed primary click or keyboard activation.
    #[must_use]
    pub fn activated(&self) -> Signal<()> {
        self.activated.clone()
    }

    /// Returns whether the button is currently pressed.
    #[must_use]
    pub const fn is_pressed(&self) -> bool {
        self.pressed
    }
}

impl Default for Button {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Button {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let child_max = size(
            (max.width() - self.padding.horizontal()).max(0.0),
            (max.height() - self.padding.vertical()).max(0.0),
        );
        let mut children = Vec::new();
        let content_size = if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(child_max))
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(
                id,
                Point::new(self.padding.left, self.padding.top),
            ));
            layout.size()
        } else {
            Size::ZERO
        };
        let size = constraints.constrain(size(
            content_size.width() + self.padding.horizontal(),
            content_size.height() + self.padding.vertical(),
        ));
        self.size = size;
        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let (normal, pressed) = self.backgrounds.unwrap_or_else(|| {
            (
                environment.theme().button_background(),
                environment.theme().button_pressed_background(),
            )
        });
        let background = if self.pressed && self.pointer_inside {
            pressed
        } else {
            normal
        };
        context.fill_rounded_rect(bounds, environment.theme().corner_radius(), background);
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() == EventPhase::Capture {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.pressed = true;
                self.pressed_pointer = Some(pointer.pointer_id);
                self.pointer_inside = true;
                context.capture_pointer(pointer.pointer_id);
                context.request_focus();
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer)
                if pointer.button == Some(PointerButton::Primary)
                    && self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                let activate = self.pressed && self.size.contains(pointer.position);
                self.pressed = false;
                self.pressed_pointer = None;
                self.pointer_inside = false;
                context.release_pointer(pointer.pointer_id);
                context.request_redraw();
                if activate {
                    self.activated.emit(&());
                }
                EventStatus::Handled
            }
            InputEvent::PointerEnter(pointer)
                if self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                self.pointer_inside = true;
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerLeave(pointer)
                if self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                self.pointer_inside = false;
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::KeyDown(key)
                if !key.repeat
                    && matches!(key.key, Key::Named(NamedKey::Enter | NamedKey::Space)) =>
            {
                self.activated.emit(&());
                context.request_redraw();
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("button layout sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use torn_core::{
        Color, Constraints, Modifiers, Point, PointerButtons, PointerEvent, PointerId,
    };
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::{LightTheme, Theme, UiRuntime};

    use super::Button;

    #[derive(Clone, Copy)]
    struct ButtonTheme;

    impl Theme for ButtonTheme {
        fn background(&self) -> Color {
            Color::BLACK
        }

        fn foreground(&self) -> Color {
            Color::WHITE
        }

        fn accent(&self) -> Color {
            Color::rgba8(30, 40, 50, 255)
        }

        fn button_background(&self) -> Color {
            Color::rgba8(10, 20, 30, 255)
        }

        fn button_pressed_background(&self) -> Color {
            Color::rgba8(40, 50, 60, 255)
        }

        fn spacing(&self) -> f32 {
            0.0
        }

        fn font_size(&self) -> f32 {
            12.0
        }

        fn corner_radius(&self) -> f32 {
            0.0
        }
    }

    #[test]
    fn resolves_normal_and_pressed_backgrounds_from_the_runtime_theme() {
        let mut runtime =
            UiRuntime::with_environment(Button::new(), torn_ui::UiEnvironment::new(ButtonTheme));
        let mut list = DisplayList::new();

        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("button layout succeeds");
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut list))
            .expect("button paint succeeds");
        assert_eq!(fill_color(&list), ButtonTheme.button_background());

        let event = torn_core::InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: torn_core::Point::ZERO,
            button: Some(torn_core::PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });
        let _ = runtime.dispatch_event(&event);
        let mut pressed_list = DisplayList::new();
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut pressed_list))
            .expect("pressed button paint succeeds");

        assert_eq!(
            fill_color(&pressed_list),
            ButtonTheme.button_pressed_background()
        );
    }

    #[test]
    fn explicit_backgrounds_override_the_runtime_theme() {
        let normal = Color::rgba8(70, 80, 90, 255);
        let pressed = Color::rgba8(100, 110, 120, 255);
        let mut button = Button::new();
        button.set_backgrounds(normal, pressed);
        let mut runtime =
            UiRuntime::with_environment(button, torn_ui::UiEnvironment::new(LightTheme));
        let mut list = DisplayList::new();

        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("button layout succeeds");
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut list))
            .expect("button paint succeeds");

        assert_eq!(fill_color(&list), normal);
    }

    #[test]
    fn activates_only_when_the_primary_pointer_is_released_inside() {
        let activations = Rc::new(Cell::new(0));
        let button = Button::new();
        button.activated().subscribe({
            let activations = Rc::clone(&activations);
            move |()| activations.set(activations.get() + 1)
        });
        let mut runtime = UiRuntime::new(button);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("button layout succeeds");

        let _ = runtime.dispatch_event(&pointer_event(Point::new(1.0, 1.0), true));
        let _ = runtime.dispatch_event(&pointer_event(Point::new(30.0, 30.0), false));
        assert_eq!(activations.get(), 0);

        let _ = runtime.dispatch_event(&pointer_event(Point::new(1.0, 1.0), true));
        let _ = runtime.dispatch_event(&pointer_event(Point::new(1.0, 1.0), false));
        assert_eq!(activations.get(), 1);
    }

    fn fill_color(list: &DisplayList) -> Color {
        let [DisplayCommand::FillRoundedRect { color, .. }] = list.commands() else {
            panic!("button should record one fill command");
        };
        *color
    }

    fn pointer_event(position: Point, pressed: bool) -> torn_core::InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position,
            button: Some(torn_core::PointerButton::Primary),
            buttons: if pressed {
                PointerButtons::PRIMARY
            } else {
                PointerButtons::NONE
            },
            modifiers: Modifiers::NONE,
        };
        if pressed {
            torn_core::InputEvent::PointerDown(event)
        } else {
            torn_core::InputEvent::PointerUp(event)
        }
    }
}
