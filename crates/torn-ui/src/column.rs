use torn_core::{Constraints, Point, Size};

use crate::{ChildLayout, LayoutResult, Widget};

/// A container that positions children top-to-bottom.
///
/// Children receive the column's maximum width and an unbounded height. The
/// column uses the largest child width and the sum of child heights, then
/// constrains that size to its own constraints. Children that exceed the
/// constrained height remain laid out at their measured sizes and may overflow
/// the column's bounds.
#[derive(Default)]
pub struct Column {
    children: Vec<Box<dyn Widget>>,
}

impl Column {
    /// Creates a column without children.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    /// Creates a column from boxed child widgets in display order.
    #[must_use]
    pub fn with_children(children: Vec<Box<dyn Widget>>) -> Self {
        Self { children }
    }

    /// Appends a child to the column.
    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    /// Returns the number of direct children.
    #[must_use]
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Returns whether this column has no children.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
}

impl Widget for Column {
    fn layout(&mut self, constraints: Constraints) -> LayoutResult {
        let child_constraints = Constraints::loose(size(constraints.max().width(), f32::INFINITY));
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        let mut children = Vec::with_capacity(self.children.len());

        for child in &mut self.children {
            let result = child.layout(child_constraints);
            let child_size = result.size();
            children.push(ChildLayout::new(Point::new(0.0, height), result));
            width = width.max(child_size.width());
            height += child_size.height();
        }

        LayoutResult::with_children(constraints.constrain(size(width, height)), children)
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("column layout sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_core::{Constraints, Point, Size};

    use crate::{Column, LayoutResult, Row, Widget};

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
    fn positions_children_vertically_and_uses_their_combined_size() {
        let mut column = Column::new();
        column.push(Fixed(size(20.0, 10.0)));
        column.push(Fixed(size(30.0, 15.0)));

        let result = column.layout(Constraints::UNBOUNDED);

        assert_eq!(result.size(), size(30.0, 25.0));
        assert_eq!(result.children()[0].origin(), Point::ZERO);
        assert_eq!(result.children()[0].bounds().size, size(20.0, 10.0));
        assert_eq!(result.children()[1].origin(), Point::new(0.0, 10.0));
        assert_eq!(result.children()[1].bounds().size, size(30.0, 15.0));
    }

    #[test]
    fn lays_out_nested_containers() {
        let mut row = Row::new();
        row.push(Fixed(size(10.0, 20.0)));
        row.push(Fixed(size(30.0, 10.0)));

        let mut column = Column::new();
        column.push(row);
        column.push(Fixed(size(15.0, 5.0)));

        let result = column.layout(Constraints::UNBOUNDED);

        assert_eq!(result.size(), size(40.0, 25.0));
        assert_eq!(result.children()[0].bounds().size, size(40.0, 20.0));
        assert_eq!(result.children()[1].origin(), Point::new(0.0, 20.0));
    }
}
