use torn_core::{Point, Size, WidgetId};

/// The layout computed for one widget.
///
/// The runtime owns the resulting bounds of every node. A widget returns only
/// its own size and the relative positions of its direct children.
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

    /// Creates a layout result with positioned direct children.
    pub fn with_children(size: Size, children: Vec<ChildLayout>) -> Self {
        Self { size, children }
    }

    /// Returns the size occupied by this widget.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Returns direct child positions in widget-tree order.
    pub fn children(&self) -> &[ChildLayout] {
        &self.children
    }
}

/// The relative position assigned to a direct child node.
#[derive(Clone, Copy, Debug, PartialEq)]
#[must_use]
pub struct ChildLayout {
    id: WidgetId,
    origin: Point,
    visible: bool,
}

impl ChildLayout {
    /// Creates a child layout at `origin` relative to its parent.
    pub const fn new(id: WidgetId, origin: Point) -> Self {
        Self {
            id,
            origin,
            visible: true,
        }
    }

    /// Creates a child layout with an explicit visibility state.
    ///
    /// Invisible children remain retained and receive layout, but are excluded
    /// from painting and hit testing until a subsequent layout makes them visible.
    pub const fn with_visibility(id: WidgetId, origin: Point, visible: bool) -> Self {
        Self {
            id,
            origin,
            visible,
        }
    }

    /// Returns the child node being positioned.
    #[must_use]
    pub const fn id(&self) -> WidgetId {
        self.id
    }

    /// Returns the child's origin relative to its parent.
    #[must_use]
    pub const fn origin(&self) -> Point {
        self.origin
    }

    /// Returns whether the child participates in painting and hit testing.
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.visible
    }
}
