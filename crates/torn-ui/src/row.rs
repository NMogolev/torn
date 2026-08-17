use torn_core::{Constraints, InputEvent, Point, Size};

use crate::{ChildLayout, EventStatus, LayoutResult, Widget, event};

/// A container that positions children left-to-right.
///
/// Children receive an unbounded width and the row's maximum height. The row
/// uses the sum of child widths and the largest child height, then constrains
/// that size to its own constraints. Children that exceed the constrained width
/// remain laid out at their measured sizes and may overflow the row's bounds.
#[derive(Default)]
pub struct Row {
    children: Vec<Box<dyn Widget>>,
    last_layout: Option<LayoutResult>,
}

impl Row {
    /// Creates a row without children.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            children: Vec::new(),
            last_layout: None,
        }
    }

    /// Creates a row from boxed child widgets in display order.
    #[must_use]
    pub fn with_children(children: Vec<Box<dyn Widget>>) -> Self {
        Self {
            children,
            last_layout: None,
        }
    }

    /// Appends a child to the row.
    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
        self.last_layout = None;
    }

    /// Returns the number of direct children.
    #[must_use]
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Returns whether this row has no children.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
}

impl Widget for Row {
    fn layout(&mut self, constraints: Constraints) -> LayoutResult {
        let child_constraints = Constraints::loose(size(f32::INFINITY, constraints.max().height()));
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        let mut children = Vec::with_capacity(self.children.len());

        for child in &mut self.children {
            let result = child.layout(child_constraints);
            let child_size = result.size();
            children.push(ChildLayout::new(Point::new(width, 0.0), result));
            width += child_size.width();
            height = height.max(child_size.height());
        }

        let result =
            LayoutResult::with_children(constraints.constrain(size(width, height)), children);
        self.last_layout = Some(result.clone());
        result
    }

    fn handle_event(&mut self, event: &InputEvent) -> EventStatus {
        let Some(position) = event::pointer_position(event) else {
            return EventStatus::Ignored;
        };
        let Some(layout) = &self.last_layout else {
            return EventStatus::Ignored;
        };

        for (child, child_layout) in self.children.iter_mut().zip(layout.children()).rev() {
            if child_layout.bounds().contains(position) {
                return child
                    .handle_event(&event::with_local_position(event, child_layout.origin()));
            }
        }

        EventStatus::Ignored
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("row layout sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_core::{Constraints, Point, Size};

    use crate::{LayoutResult, Row, Widget};

    struct Fixed(Size);

    impl Widget for Fixed {
        fn layout(&mut self, constraints: Constraints) -> LayoutResult {
            LayoutResult::new(constraints.constrain(self.0))
        }
    }

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn positions_children_horizontally_and_uses_their_combined_size() {
        let mut row = Row::new();
        row.push(Fixed(size(20.0, 10.0)));
        row.push(Fixed(size(30.0, 15.0)));

        let result = row.layout(Constraints::UNBOUNDED);

        assert_eq!(result.size(), size(50.0, 15.0));
        assert_eq!(result.children()[0].origin(), Point::ZERO);
        assert_eq!(result.children()[0].bounds().size, size(20.0, 10.0));
        assert_eq!(result.children()[1].origin(), Point::new(20.0, 0.0));
        assert_eq!(result.children()[1].bounds().size, size(30.0, 15.0));
    }

    #[test]
    fn constrains_child_cross_axis_and_own_size() {
        let mut row = Row::new();
        row.push(Fixed(size(40.0, 30.0)));

        let constraints =
            Constraints::new(size(50.0, 0.0), size(50.0, 20.0)).expect("valid test constraints");
        let result = row.layout(constraints);

        assert_eq!(result.size(), size(50.0, 20.0));
        assert_eq!(result.children()[0].bounds().size, size(40.0, 20.0));
    }
}
