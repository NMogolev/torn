use std::boxed::Box as HeapBox;

use torn_core::{Color, Constraints, InputEvent, Insets, Point, PointerButton, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, UiEnvironment,
    Widget,
};

/// A clickable single-child control with a rectangular background.
///
/// Its direct child is owned by [`torn_ui::UiRuntime`] and should be appended to
/// the button node before layout.
pub struct Button {
    backgrounds: Option<(Color, Color)>,
    on_click: Option<HeapBox<dyn FnMut()>>,
    padding: Insets,
    pressed: bool,
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
            on_click: None,
            padding: Insets::all(8.0),
            pressed: false,
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
        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        let (normal, pressed) = self.backgrounds.unwrap_or_else(|| {
            (
                environment.theme().button_background(),
                environment.theme().button_pressed_background(),
            )
        });
        let background = if self.pressed { pressed } else { normal };
        context.fill_rounded_rect(bounds, environment.theme().corner_radius(), background);
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() == EventPhase::Capture {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.pressed = true;
                context.capture_pointer(pointer.pointer_id);
                context.request_focus();
                context.request_redraw();
                if let Some(callback) = &mut self.on_click {
                    callback();
                }
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.pressed = false;
                context.release_pointer(pointer.pointer_id);
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
    use torn_core::{Color, Constraints, Modifiers, PointerButtons, PointerEvent, PointerId};
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

    fn fill_color(list: &DisplayList) -> Color {
        let [DisplayCommand::FillRoundedRect { color, .. }] = list.commands() else {
            panic!("button should record one fill command");
        };
        *color
    }
}
