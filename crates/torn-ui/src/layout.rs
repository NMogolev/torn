use torn_core::{Point, Rect, Size};

/// The layout computed for one widget and its descendants.
#[derive(Clone, Debug, PartialEq)]
#[must_use]
pub struct LayoutResult {
    size: Size,
    children: Vec<ChildLayout>,
}

impl LayoutResult {
    /// Creates a layout result for a widget without children.
    pub const fn new(size: Size) -> Self {
        Self {
            size,
            children: Vec::new(),
        }
    }

    /// Creates a layout result with positioned child layouts.
    pub fn with_children(size: Size, children: Vec<ChildLayout>) -> Self {
        Self { size, children }
    }

    /// Returns the size occupied by this widget.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Returns child layouts in their widget-tree order.
    pub fn children(&self) -> &[ChildLayout] {
        &self.children
    }
}

/// The position and layout result of a direct child widget.
#[derive(Clone, Debug, PartialEq)]
#[must_use]
pub struct ChildLayout {
    origin: Point,
    result: LayoutResult,
}

impl ChildLayout {
    /// Creates a child layout at `origin` relative to its parent.
    pub const fn new(origin: Point, result: LayoutResult) -> Self {
        Self { origin, result }
    }

    /// Returns the child's origin relative to its parent.
    #[must_use]
    pub const fn origin(&self) -> Point {
        self.origin
    }

    /// Returns the child's bounds relative to its parent.
    #[must_use]
    pub const fn bounds(&self) -> Rect {
        Rect::new(self.origin, self.result.size)
    }

    /// Returns the layout result computed for the child.
    pub const fn result(&self) -> &LayoutResult {
        &self.result
    }
}
