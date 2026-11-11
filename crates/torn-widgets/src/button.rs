use torn_core::{
    Color, Constraints, InputEvent, Insets, Key, NamedKey, Point, PointerButton, Rect, Size,
};
use torn_render::PaintContext;
use torn_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext,
    LayoutResult, PseudoClass, Signal, StyleTarget, SurfaceStyle, TssStyle, UiEnvironment, Widget,
};

/// Locally overridable visual properties for a [`Button`].
///
/// Missing values are resolved from the runtime theme. The surface's `padding`,
/// `corner_radius`, and `border` apply in every interaction state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButtonStyle {
    /// Shared surface appearance for every button state.
    pub surface: SurfaceStyle,
    /// Background while the button is idle.
    pub background: Option<Color>,
    /// Background while a pointer hovers over the button.
    pub hover_background: Option<Color>,
    /// Background while the primary pointer presses the button.
    pub pressed_background: Option<Color>,
}

/// A clickable single-child control with a rectangular background.
///
/// Its direct child is owned by [`torn_ui::UiRuntime`] and should be appended to
/// the button node before layout.
pub struct Button {
    style: ButtonStyle,
    activated: Signal<()>,
    pressed: bool,
    pressed_pointer: Option<torn_core::PointerId>,
    pointer_inside: bool,
    size: Size,
}

impl Button {
    /// Creates an empty button whose appearance is resolved from the runtime theme.
    ///
    /// Its colors are resolved from the runtime [`torn_ui::Theme`] until
    /// [`Self::set_backgrounds`] or a local [`ButtonStyle`] is supplied.
    #[must_use]
    pub fn new() -> Self {
        Self {
            style: ButtonStyle::default(),
            activated: Signal::new(),
            pressed: false,
            pressed_pointer: None,
            pointer_inside: false,
            size: Size::ZERO,
        }
    }

    /// Sets the button's normal and pressed background colors.
    pub fn set_backgrounds(&mut self, normal: Color, pressed: Color) {
        self.style.background = Some(normal);
        self.style.pressed_background = Some(pressed);
    }

    /// Sets the inset between the button edge and its child.
    pub fn set_padding(&mut self, padding: Insets) {
        self.style.surface.padding = Some(padding);
    }

    /// Replaces the button's local visual style.
    pub fn set_style(&mut self, style: ButtonStyle) {
        self.style = style;
    }

    /// Returns the button's local visual style.
    #[must_use]
    pub const fn style(&self) -> ButtonStyle {
        self.style
    }

    /// Returns this button with `style` applied locally.
    #[must_use]
    pub fn with_style(mut self, style: ButtonStyle) -> Self {
        self.set_style(style);
        self
    }

    /// Returns this button with a normal background color.
    #[must_use]
    pub fn with_background(mut self, color: Color) -> Self {
        self.style.background = Some(color);
        self
    }

    /// Returns this button with a hover background color.
    #[must_use]
    pub fn with_hover_background(mut self, color: Color) -> Self {
        self.style.hover_background = Some(color);
        self
    }

    /// Returns this button with a pressed background color.
    #[must_use]
    pub fn with_pressed_background(mut self, color: Color) -> Self {
        self.style.pressed_background = Some(color);
        self
    }

    /// Returns this button with a border.
    #[must_use]
    pub fn with_border(mut self, border: torn_ui::Border) -> Self {
        self.style.surface.border = Some(border);
        self
    }

    /// Returns this button with rounded corners of `radius` logical pixels.
    #[must_use]
    pub fn with_corner_radius(mut self, radius: f32) -> Self {
        self.style.surface.corner_radius = Some(radius);
        self
    }

    /// Returns this button with an inset between its edge and direct child.
    #[must_use]
    pub fn with_padding(mut self, padding: Insets) -> Self {
        self.style.surface.padding = Some(padding);
        self
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
        let style = context.style();
        let padding = self
            .style
            .surface
            .padding
            .or(style.padding)
            .unwrap_or_else(|| context.environment().theme().control_padding());
        let max = constraints.max();
        let child_max = size(
            (max.width() - padding.horizontal()).max(0.0),
            (max.height() - padding.vertical()).max(0.0),
        );
        let mut children = Vec::new();
        let content_size = if context.child_count() == 1 {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(child_max))
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(id, Point::new(padding.left, padding.top)));
            layout.size()
        } else {
            Size::ZERO
        };
        let size = constraints.constrain(size(
            content_size.width() + padding.horizontal(),
            content_size.height() + padding.vertical(),
        ));
        self.size = size;
        LayoutResult::with_children(size, children)
    }

    fn style_target(&self) -> StyleTarget {
        let target = StyleTarget::new("button");
        if self.pressed {
            target.with_state(PseudoClass::Pressed)
        } else {
            target
        }
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
        let styled_idle = self.style.background.or(style.background);
        let idle = styled_idle.unwrap_or_else(|| theme.control_background());
        let hover = self
            .style
            .hover_background
            .or(styled_idle)
            .unwrap_or_else(|| theme.control_hover_background());
        let pressed = self
            .style
            .pressed_background
            .or(styled_idle)
            .unwrap_or_else(|| theme.control_pressed_background());
        let background = context.transition().control_color(idle, hover, pressed);
        let radius = self
            .style
            .surface
            .corner_radius
            .or(style.corner_radius)
            .unwrap_or_else(|| theme.corner_radius());
        context.fill_rounded_rect(bounds, radius, background);
        if let Some(border) = self.style.surface.border.or(style.border) {
            context.stroke_rounded_rect(bounds, radius, border.width, border.color);
        }
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
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
            InputEvent::PointerCancel(pointer)
                if self.pressed_pointer == Some(pointer.pointer_id) =>
            {
                self.pressed = false;
                self.pressed_pointer = None;
                self.pointer_inside = false;
                context.release_pointer(pointer.pointer_id);
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerEnter(_) => {
                self.pointer_inside = true;
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerLeave(_) => {
                self.pointer_inside = false;
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::KeyDown(key)
                if !key.repeat
                    && context.current_target() == context.target()
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

    fn accessibility(&self) -> Accessibility {
        Accessibility::new(AccessibilityRole::Button)
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(AccessibilityActions::FOCUS | AccessibilityActions::CLICK)
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        if matches!(action, AccessibilityAction::Click) {
            self.activated.emit(&());
            true
        } else {
            false
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
        Color, Constraints, InputEvent, Key, KeyCode, Modifiers, NamedKey, Point, PointerButtons,
        PointerEvent, PointerId, Size,
    };
    use torn_render::{DisplayCommand, DisplayList, FontdueTextShaper, TextStyle};
    use torn_ui::{EventStatus, LightTheme, Theme, UiRuntime};

    use crate::{Checkbox, Text};

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

        fn control_background(&self) -> Color {
            Color::rgba8(10, 20, 30, 255)
        }

        fn control_pressed_background(&self) -> Color {
            Color::rgba8(40, 50, 60, 255)
        }

        fn control_hover_background(&self) -> Color {
            Color::rgba8(25, 35, 45, 255)
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
    fn blends_hover_color_over_the_transition_duration() {
        let mut runtime =
            UiRuntime::with_environment(Button::new(), torn_ui::UiEnvironment::new(ButtonTheme));
        runtime
            .layout(
                Constraints::tight(Size::new(20.0, 20.0).expect("valid button size"))
                    .expect("valid constraints"),
            )
            .expect("button layout succeeds");
        let _ = runtime.dispatch_event(&pointer_move(Point::new(10.0, 10.0)));
        runtime.advance_animations(ButtonTheme.animation_duration() / 2);
        let mut list_mid = DisplayList::new();
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut list_mid))
            .expect("button paint succeeds");

        let color = fill_color(&list_mid);
        let expected = ButtonTheme
            .control_background()
            .mix(ButtonTheme.control_hover_background(), 0.5);
        assert!((color.red - expected.red).abs() < 0.01);
        assert!((color.green - expected.green).abs() < 0.01);
        assert!((color.blue - expected.blue).abs() < 0.01);
        assert_ne!(color, ButtonTheme.control_background());
        assert_ne!(color, ButtonTheme.control_hover_background());
    }

    #[test]
    fn resolves_normal_and_pressed_backgrounds_from_the_runtime_theme() {
        let mut runtime =
            UiRuntime::with_environment(Button::new(), torn_ui::UiEnvironment::new(ButtonTheme));
        let mut list = DisplayList::new();

        runtime
            .layout(
                Constraints::tight(Size::new(20.0, 20.0).expect("valid button size"))
                    .expect("valid constraints"),
            )
            .expect("button layout succeeds");
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut list))
            .expect("button paint succeeds");
        assert_eq!(fill_color(&list), ButtonTheme.control_background());

        let event = torn_core::InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: torn_core::Point::ZERO,
            button: Some(torn_core::PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });
        let _ = runtime.dispatch_event(&event);
        runtime.settle_animations();
        let mut pressed_list = DisplayList::new();
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut pressed_list))
            .expect("pressed button paint succeeds");

        assert_eq!(
            fill_color(&pressed_list),
            ButtonTheme.control_pressed_background()
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
            .layout(
                Constraints::tight(Size::new(20.0, 20.0).expect("valid button size"))
                    .expect("valid constraints"),
            )
            .expect("button layout succeeds");
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut list))
            .expect("button paint succeeds");

        assert_eq!(fill_color(&list), normal);
    }

    #[test]
    fn local_style_overrides_theme_for_hovered_and_pressed_states() {
        let normal = Color::rgba8(70, 80, 90, 255);
        let hover = Color::rgba8(100, 110, 120, 255);
        let pressed = Color::rgba8(130, 140, 150, 255);
        let button = Button::new()
            .with_background(normal)
            .with_hover_background(hover)
            .with_pressed_background(pressed)
            .with_corner_radius(9.0);
        let mut runtime =
            UiRuntime::with_environment(button, torn_ui::UiEnvironment::new(ButtonTheme));

        runtime
            .layout(
                Constraints::tight(Size::new(20.0, 20.0).expect("valid button size"))
                    .expect("valid constraints"),
            )
            .expect("button layout succeeds");
        let _ = runtime.dispatch_event(&pointer_move(Point::ZERO));
        runtime.settle_animations();
        let mut hover_list = DisplayList::new();
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut hover_list))
            .expect("hovered button paint succeeds");
        assert_eq!(fill_color(&hover_list), hover);
        assert!((fill_radius(&hover_list) - 9.0).abs() < f32::EPSILON);

        let _ = runtime.dispatch_event(&InputEvent::PointerLeave(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(30.0, 30.0),
            button: None,
            buttons: PointerButtons::NONE,
            modifiers: Modifiers::NONE,
        }));
        runtime.settle_animations();
        let mut normal_list = DisplayList::new();
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut normal_list))
            .expect("button paint after pointer leave succeeds");
        assert_eq!(fill_color(&normal_list), normal);

        let _ = runtime.dispatch_event(&pointer_event(Point::ZERO, true));
        runtime.settle_animations();
        let mut pressed_list = DisplayList::new();
        runtime
            .paint(&mut torn_render::PaintContext::new(&mut pressed_list))
            .expect("pressed button paint succeeds");
        assert_eq!(fill_color(&pressed_list), pressed);
    }

    #[test]
    fn activates_only_when_the_primary_pointer_is_released_inside() {
        let activations = Rc::new(Cell::new(0));
        let button = Button::new();
        let _subscription = button.activated().subscribe({
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
        let _ = runtime.dispatch_event(&InputEvent::WindowFocusChanged(false));
        let _ = runtime.dispatch_event(&pointer_event(Point::new(1.0, 1.0), false));
        assert_eq!(activations.get(), 0);

        let _ = runtime.dispatch_event(&pointer_event(Point::new(1.0, 1.0), true));
        let _ = runtime.dispatch_event(&pointer_event(Point::new(1.0, 1.0), false));
        assert_eq!(activations.get(), 1);
    }

    #[test]
    fn lets_interactive_children_receive_pointer_and_keyboard_input() {
        let activations = Rc::new(Cell::new(0));
        let button = Button::new();
        let _button_subscription = button.activated().subscribe({
            let activations = Rc::clone(&activations);
            move |()| activations.set(activations.get() + 1)
        });
        let changes = Rc::new(Cell::new(0));
        let checkbox = Checkbox::new();
        let _checkbox_subscription = checkbox.toggled().subscribe({
            let changes = Rc::clone(&changes);
            move |_| changes.set(changes.get() + 1)
        });
        let mut runtime = UiRuntime::new(button);
        let checkbox = runtime
            .append_child(runtime.root(), checkbox)
            .expect("button exists");
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("button layout succeeds");
        let position = runtime
            .bounds(checkbox)
            .expect("checkbox remains live")
            .origin;

        assert_eq!(
            runtime.dispatch_event(&pointer_event(position, true)),
            EventStatus::Handled
        );
        assert_eq!(
            runtime.dispatch_event(&pointer_event(position, false)),
            EventStatus::Handled
        );
        assert_eq!(runtime.focused_widget(), Some(checkbox));
        assert_eq!(changes.get(), 1);
        assert_eq!(activations.get(), 0);

        assert_eq!(
            runtime.dispatch_event(&key(NamedKey::Space)),
            EventStatus::Handled
        );
        assert_eq!(changes.get(), 2);
        assert_eq!(activations.get(), 0);

        assert_eq!(
            runtime.dispatch_event(&key(NamedKey::Enter)),
            EventStatus::Handled
        );
        assert_eq!(changes.get(), 3);
        assert_eq!(activations.get(), 0);
    }

    #[test]
    fn treats_decorative_text_as_transparent_to_pointer_hit_testing() {
        let activations = Rc::new(Cell::new(0));
        let button = Button::new();
        let _subscription = button.activated().subscribe({
            let activations = Rc::clone(&activations);
            move |()| activations.set(activations.get() + 1)
        });
        let mut runtime = UiRuntime::new(button);
        let label = runtime
            .append_child(
                runtime.root(),
                Text::new(FontdueTextShaper::ubuntu_light().layout(
                    "Нажать",
                    &TextStyle::new(16.0, Color::BLACK),
                    None,
                )),
            )
            .expect("button exists");
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("button layout succeeds");
        let position = runtime.bounds(label).expect("label remains live").origin;

        assert_eq!(
            runtime.dispatch_event(&pointer_event(position, true)),
            EventStatus::Handled
        );
        assert_eq!(
            runtime.dispatch_event(&pointer_event(position, false)),
            EventStatus::Handled
        );
        assert_eq!(activations.get(), 1);
    }

    fn fill_color(list: &DisplayList) -> Color {
        list.commands()
            .iter()
            .find_map(|command| match command {
                DisplayCommand::FillRoundedRect { color, .. } => Some(*color),
                _ => None,
            })
            .expect("button should record a rounded fill command")
    }

    fn fill_radius(list: &DisplayList) -> f32 {
        list.commands()
            .iter()
            .find_map(|command| match command {
                DisplayCommand::FillRoundedRect { radius, .. } => Some(*radius),
                _ => None,
            })
            .expect("button should record a rounded fill command")
    }

    fn pointer_move(position: Point) -> InputEvent {
        InputEvent::PointerMove(PointerEvent {
            pointer_id: PointerId(1),
            position,
            button: None,
            buttons: PointerButtons::NONE,
            modifiers: Modifiers::NONE,
        })
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

    fn key(key: NamedKey) -> InputEvent {
        InputEvent::KeyDown(torn_core::KeyEvent {
            key: Key::Named(key),
            code: KeyCode::Unidentified,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
    }
}
