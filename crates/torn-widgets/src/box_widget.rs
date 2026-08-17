use std::boxed::Box as HeapBox;

use torn_core::{Color, Constraints, InputEvent, Point, Rect, Size};
use torn_render::PaintContext;
use torn_ui::{ChildLayout, EventStatus, LayoutResult, Widget};

/// A single-child container similar to an HTML `div`.
///
/// `Box` can paint a background and expands to satisfy its own constraints.
/// Its child is placed at the top-left corner without padding or alignment.
#[derive(Default)]
pub struct Box {
    background: Option<Color>,
    child: Option<HeapBox<dyn Widget>>,
    last_layout: Option<LayoutResult>,
}

impl Box {
    /// Creates an empty transparent box.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            background: None,
            child: None,
            last_layout: None,
        }
    }

    /// Creates a transparent box containing `child`.
    #[must_use]
    pub fn with_child(child: impl Widget + 'static) -> Self {
        Self {
            background: None,
            child: Some(HeapBox::new(child)),
            last_layout: None,
        }
    }

    /// Sets the optional background color.
    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
    }

    /// Replaces this box's child.
    pub fn set_child(&mut self, child: impl Widget + 'static) {
        self.child = Some(HeapBox::new(child));
        self.last_layout = None;
    }

    /// Removes and returns the child, if one exists.
    pub fn take_child(&mut self) -> Option<HeapBox<dyn Widget>> {
        self.last_layout = None;
        self.child.take()
    }
}

impl Widget for Box {
    fn layout(&mut self, constraints: Constraints) -> LayoutResult {
        let child_constraints = Constraints::loose(constraints.max());
        let children = self.child.as_mut().map_or_else(Vec::new, |child| {
            vec![ChildLayout::new(
                Point::ZERO,
                child.layout(child_constraints),
            )]
        });
        let content_size = children
            .first()
            .map_or(Size::ZERO, |child| child.result().size());
        let result = LayoutResult::with_children(constraints.constrain(content_size), children);
        self.last_layout = Some(result.clone());
        result
    }

    fn paint(&self, context: &mut PaintContext<'_>, origin: Point) {
        let Some(layout) = &self.last_layout else {
            return;
        };
        if let Some(background) = self.background {
            context.fill_rect(Rect::new(origin, layout.size()), background);
        }
        if let (Some(child), Some(child_layout)) = (&self.child, layout.children().first()) {
            child.paint(
                context,
                Point::new(
                    origin.x + child_layout.origin().x,
                    origin.y + child_layout.origin().y,
                ),
            );
        }
    }

    fn handle_event(&mut self, event: &InputEvent) -> EventStatus {
        let Some(position) = pointer_position(event) else {
            return EventStatus::Ignored;
        };
        let (Some(child), Some(layout)) = (&mut self.child, &self.last_layout) else {
            return EventStatus::Ignored;
        };
        let Some(child_layout) = layout.children().first() else {
            return EventStatus::Ignored;
        };
        if child_layout.bounds().contains(position) {
            return child.handle_event(&with_local_position(event, child_layout.origin()));
        }

        EventStatus::Ignored
    }
}

fn pointer_position(event: &InputEvent) -> Option<Point> {
    match event {
        InputEvent::PointerDown(event)
        | InputEvent::PointerMove(event)
        | InputEvent::PointerUp(event) => Some(event.position),
        InputEvent::Wheel(_)
        | InputEvent::KeyDown(_)
        | InputEvent::KeyUp(_)
        | InputEvent::TextInput(_)
        | InputEvent::FocusChanged(_) => None,
    }
}

fn with_local_position(event: &InputEvent, origin: Point) -> InputEvent {
    let mut event = event.clone();
    match &mut event {
        InputEvent::PointerDown(pointer)
        | InputEvent::PointerMove(pointer)
        | InputEvent::PointerUp(pointer) => {
            pointer.position.x -= origin.x;
            pointer.position.y -= origin.y;
        }
        InputEvent::Wheel(_)
        | InputEvent::KeyDown(_)
        | InputEvent::KeyUp(_)
        | InputEvent::TextInput(_)
        | InputEvent::FocusChanged(_) => {}
    }
    event
}

#[cfg(test)]
mod tests {
    use torn_core::{Color, Constraints, Point, Rect, Size};
    use torn_render::{DisplayCommand, DisplayList, PaintContext};
    use torn_ui::{LayoutResult, Widget};

    use super::Box;

    struct ColoredLeaf {
        size: Size,
        color: Color,
    }

    impl Widget for ColoredLeaf {
        fn layout(&mut self, constraints: Constraints) -> LayoutResult {
            LayoutResult::new(constraints.constrain(self.size))
        }

        fn paint(&self, context: &mut PaintContext<'_>, origin: Point) {
            context.fill_rect(Rect::new(origin, self.size), self.color);
        }
    }

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn paints_background_before_its_child() {
        let mut container = Box::with_child(ColoredLeaf {
            size: size(20.0, 10.0),
            color: Color::WHITE,
        });
        container.set_background(Some(Color::BLACK));
        let mut list = DisplayList::new();

        assert_eq!(
            container.layout(Constraints::UNBOUNDED).size(),
            size(20.0, 10.0)
        );
        container.paint(&mut PaintContext::new(&mut list), Point::new(3.0, 4.0));

        assert_eq!(
            list.commands(),
            &[
                DisplayCommand::FillRect {
                    rect: Rect::new(Point::new(3.0, 4.0), size(20.0, 10.0)),
                    color: Color::BLACK,
                },
                DisplayCommand::FillRect {
                    rect: Rect::new(Point::new(3.0, 4.0), size(20.0, 10.0)),
                    color: Color::WHITE,
                },
            ]
        );
    }
}
