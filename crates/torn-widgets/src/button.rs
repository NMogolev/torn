use std::boxed::Box as HeapBox;

use torn_core::{Color, Constraints, InputEvent, Insets, Point, PointerButton, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Widget,
};

/// A clickable single-child control with a rectangular background.
///
/// Its direct child is owned by [`torn_ui::UiRuntime`] and should be appended to
/// the button node before layout.
pub struct Button {
    background: Color,
    pressed_background: Color,
    on_click: Option<HeapBox<dyn FnMut()>>,
    padding: Insets,
    pressed: bool,
    size: Size,
}

impl Button {
    /// Creates an empty button with an 8-pixel inset on every edge.
    #[must_use]
    pub fn new() -> Self {
        Self {
            background: Color::rgba8(235, 235, 235, 255),
            pressed_background: Color::rgba8(210, 210, 210, 255),
            on_click: None,
            padding: Insets::all(8.0),
            pressed: false,
            size: Size::ZERO,
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
        self.size = constraints.constrain(size(
            content_size.width() + self.padding.horizontal(),
            content_size.height() + self.padding.vertical(),
        ));
        LayoutResult::with_children(self.size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, origin: Point) {
        let background = if self.pressed {
            self.pressed_background
        } else {
            self.background
        };
        context.fill_rect(Rect::new(origin, self.size), background);
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
