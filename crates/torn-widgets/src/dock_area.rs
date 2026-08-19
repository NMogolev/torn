use std::{cell::RefCell, rc::Rc};

use torn_core::{Color, Constraints, InputEvent, Point, PointerButton, PointerId, Rect, Size};
use torn_render::{FontdueTextShaper, PaintContext, TextStyle};
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, UiEnvironment,
    Widget,
};
use torn_workspace::{DockAxis, DocumentId, LayoutNode, PanelId, WorkspaceLayout};

const SPLITTER_THICKNESS: f32 = 4.0;
const TAB_BAR_HEIGHT: f32 = 30.0;
const TAB_PADDING: f32 = 10.0;

/// Stable workspace item represented by one direct child of a [`DockArea`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DockItem {
    /// A tool panel identified by [`PanelId`].
    Panel(PanelId),
    /// A document identified by [`DocumentId`].
    Document(DocumentId),
}

/// Why adding an item binding to a [`DockArea`] failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DockAreaError {
    /// The item already has a direct-child binding.
    DuplicateItem(DockItem),
}

impl core::fmt::Display for DockAreaError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateItem(item) => write!(formatter, "dock item `{item:?}` is already bound"),
        }
    }
}

impl std::error::Error for DockAreaError {}

/// Projects a persistent [`WorkspaceLayout`] onto retained direct-child widgets.
///
/// The application owns the shared layout and updates it with the transaction
/// methods on [`WorkspaceLayout`]. Call [`torn_ui::UiRuntime::layout`] after a
/// change to project the new tree. Each registered item corresponds to the
/// direct child at the same index, so register all items before appending their
/// widgets to the runtime. Inactive tabs, hidden items, and floating items stay
/// retained but are not painted or hit tested by this dock area.
pub struct DockArea {
    layout: Rc<RefCell<WorkspaceLayout>>,
    items: Vec<DockItem>,
    missing_items: Vec<Rect>,
    splitters: Vec<Splitter>,
    tabs: Vec<Tab>,
    dragging: Option<SplitterDrag>,
}

#[derive(Clone, Debug)]
struct Splitter {
    bounds: Rect,
    resize_panel: Option<PanelId>,
    axis: DockAxis,
    content_origin: f32,
    content_length: f32,
}

#[derive(Clone, Debug)]
struct SplitterDrag {
    pointer_id: PointerId,
    splitter: Splitter,
}

#[derive(Clone, Debug)]
struct Tab {
    bounds: Rect,
    item: DockItem,
    active: bool,
}

impl DockArea {
    /// Creates a projector for the shared workspace layout.
    #[must_use]
    pub fn new(layout: Rc<RefCell<WorkspaceLayout>>) -> Self {
        Self {
            layout,
            items: Vec::new(),
            missing_items: Vec::new(),
            splitters: Vec::new(),
            tabs: Vec::new(),
            dragging: None,
        }
    }

    /// Returns the layout projected by this area.
    #[must_use]
    pub fn workspace_layout(&self) -> &Rc<RefCell<WorkspaceLayout>> {
        &self.layout
    }

    /// Returns item bindings in direct-child order.
    #[must_use]
    pub fn items(&self) -> &[DockItem] {
        &self.items
    }

    /// Binds the next direct child to `panel`.
    ///
    /// Append that child to the `DockArea` in the runtime after all bindings
    /// have been registered.
    ///
    /// # Errors
    ///
    /// Returns [`DockAreaError::DuplicateItem`] when `panel` is already bound.
    pub fn register_panel(&mut self, panel: PanelId) -> Result<(), DockAreaError> {
        self.register_item(DockItem::Panel(panel))
    }

    /// Binds the next direct child to `document`.
    ///
    /// Append that child to the `DockArea` in the runtime after all bindings
    /// have been registered.
    ///
    /// # Errors
    ///
    /// Returns [`DockAreaError::DuplicateItem`] when `document` is already bound.
    pub fn register_document(&mut self, document: DocumentId) -> Result<(), DockAreaError> {
        self.register_item(DockItem::Document(document))
    }

    fn register_item(&mut self, item: DockItem) -> Result<(), DockAreaError> {
        if self.items.contains(&item) {
            return Err(DockAreaError::DuplicateItem(item));
        }
        self.items.push(item);
        Ok(())
    }
}

impl Widget for DockArea {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let root = self.layout.borrow().root().clone();
        let mut item_bounds = vec![None; self.items.len()];
        self.missing_items.clear();
        self.splitters.clear();
        self.tabs.clear();
        project_node(
            &root,
            Rect::new(Point::ZERO, size),
            &self.items,
            &mut item_bounds,
            &mut self.missing_items,
            &mut self.splitters,
            &mut self.tabs,
        );

        let mut children = Vec::with_capacity(context.child_count());
        for index in 0..context.child_count() {
            let bounds = item_bounds.get(index).copied().flatten();
            let child_constraints = bounds.map_or(Constraints::ZERO, |rect| {
                Constraints::tight(rect.size).expect("projected dock bounds are finite")
            });
            let (id, _) = context
                .layout_child(index, child_constraints)
                .expect("runtime child index is valid");
            let origin = bounds.map_or(Point::ZERO, |rect| rect.origin);
            children.push(ChildLayout::with_visibility(id, origin, bounds.is_some()));
        }

        LayoutResult::with_children(size, children)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, environment.theme().background());
        let splitter_color = environment.theme().button_background();
        for splitter in &self.splitters {
            context.fill_rect(offset_rect(splitter.bounds, bounds.origin), splitter_color);
        }
        for tab in &self.tabs {
            let color = if tab.active {
                environment.theme().accent()
            } else {
                environment.theme().button_background()
            };
            let tab_bounds = offset_rect(tab.bounds, bounds.origin);
            context.fill_rect(tab_bounds, color);
            let label = match &tab.item {
                DockItem::Panel(id) => id.as_str(),
                DockItem::Document(id) => id.as_str(),
            };
            let text = FontdueTextShaper::ubuntu_light().layout(
                label,
                &TextStyle::new(12.0, environment.theme().foreground()),
                Some((tab_bounds.size.width() - TAB_PADDING * 2.0).max(0.0)),
            );
            context.draw_text(
                text,
                Point::new(tab_bounds.origin.x + TAB_PADDING, tab_bounds.origin.y + 8.0),
            );
        }
        for missing in &self.missing_items {
            context.fill_rect(
                offset_rect(*missing, bounds.origin),
                Color::rgba(0.8, 0.2, 0.2, 1.0),
            );
        }
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }

        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                if let Some(tab) = self
                    .tabs
                    .iter()
                    .find(|tab| tab.bounds.contains(pointer.position))
                {
                    let result = match &tab.item {
                        DockItem::Panel(panel) => self.layout.borrow_mut().activate_panel(panel),
                        DockItem::Document(document) => {
                            self.layout.borrow_mut().activate_document(document)
                        }
                    };
                    if result.is_ok() {
                        context.request_redraw();
                        return EventStatus::Handled;
                    }
                }
                let Some(splitter) = self
                    .splitters
                    .iter()
                    .find(|splitter| splitter.bounds.contains(pointer.position))
                    .cloned()
                else {
                    return EventStatus::Ignored;
                };
                if splitter.resize_panel.is_none() || splitter.content_length <= 0.0 {
                    return EventStatus::Ignored;
                }
                self.dragging = Some(SplitterDrag {
                    pointer_id: pointer.pointer_id,
                    splitter,
                });
                context.capture_pointer(pointer.pointer_id);
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerMove(pointer) => {
                let Some(dragging) = &self.dragging else {
                    return EventStatus::Ignored;
                };
                if dragging.pointer_id != pointer.pointer_id {
                    return EventStatus::Ignored;
                }
                let coordinate = match dragging.splitter.axis {
                    DockAxis::Horizontal => pointer.position.x,
                    DockAxis::Vertical => pointer.position.y,
                };
                let ratio = (coordinate - dragging.splitter.content_origin)
                    / dragging.splitter.content_length;
                if let Some(panel) = &dragging.splitter.resize_panel {
                    let _ = self
                        .layout
                        .borrow_mut()
                        .resize_split_for_panel(panel, ratio);
                    context.request_redraw();
                    EventStatus::Handled
                } else {
                    EventStatus::Ignored
                }
            }
            InputEvent::PointerUp(pointer) if pointer.button == Some(PointerButton::Primary) => {
                let Some(dragging) = &self.dragging else {
                    return EventStatus::Ignored;
                };
                if dragging.pointer_id != pointer.pointer_id {
                    return EventStatus::Ignored;
                }
                self.dragging = None;
                context.release_pointer(pointer.pointer_id);
                context.request_redraw();
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }
}

fn available_size(constraints: Constraints) -> Size {
    let max = constraints.max();
    let min = constraints.min();
    let width = if max.width().is_finite() {
        max.width()
    } else {
        min.width()
    };
    let height = if max.height().is_finite() {
        max.height()
    } else {
        min.height()
    };
    constraints.constrain(Size::new(width, height).expect("constraint extents are valid"))
}

fn project_node(
    node: &LayoutNode,
    bounds: Rect,
    items: &[DockItem],
    item_bounds: &mut [Option<Rect>],
    missing_items: &mut Vec<Rect>,
    splitters: &mut Vec<Splitter>,
    tabs: &mut Vec<Tab>,
) {
    match node {
        LayoutNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let (first_bounds, splitter, second_bounds) = split_bounds(bounds, *axis, *ratio);
            let (content_origin, content_length) = match axis {
                DockAxis::Horizontal => (
                    bounds.origin.x,
                    (bounds.size.width() - splitter.size.width()).max(0.0),
                ),
                DockAxis::Vertical => (
                    bounds.origin.y,
                    (bounds.size.height() - splitter.size.height()).max(0.0),
                ),
            };
            splitters.push(Splitter {
                bounds: splitter,
                resize_panel: first_panel(first).or_else(|| first_panel(second)),
                axis: *axis,
                content_origin,
                content_length,
            });
            project_node(
                first,
                first_bounds,
                items,
                item_bounds,
                missing_items,
                splitters,
                tabs,
            );
            project_node(
                second,
                second_bounds,
                items,
                item_bounds,
                missing_items,
                splitters,
                tabs,
            );
        }
        LayoutNode::Tabs {
            items: tab_items,
            active,
        } => project_tabs(
            tab_items.iter().cloned().map(DockItem::Panel),
            active.as_ref().map(|panel| DockItem::Panel(panel.clone())),
            bounds,
            items,
            item_bounds,
            missing_items,
            tabs,
        ),
        LayoutNode::Panel { id } => project_item(
            &DockItem::Panel(id.clone()),
            bounds,
            items,
            item_bounds,
            missing_items,
        ),
        LayoutNode::Documents {
            items: document_items,
            active,
            ..
        } => project_tabs(
            document_items.iter().cloned().map(DockItem::Document),
            active
                .as_ref()
                .map(|document| DockItem::Document(document.clone())),
            bounds,
            items,
            item_bounds,
            missing_items,
            tabs,
        ),
        LayoutNode::Empty => {}
    }
}

fn project_tabs(
    tab_items: impl IntoIterator<Item = DockItem>,
    active: Option<DockItem>,
    bounds: Rect,
    items: &[DockItem],
    item_bounds: &mut [Option<Rect>],
    missing_items: &mut Vec<Rect>,
    tabs: &mut Vec<Tab>,
) {
    let tab_items = tab_items.into_iter().collect::<Vec<_>>();
    if tab_items.is_empty() {
        return;
    }
    let tab_height = bounds.size.height().min(TAB_BAR_HEIGHT);
    let tab_count = u16::try_from(tab_items.len()).expect("workspace tab count fits in u16");
    let tab_width = bounds.size.width() / f32::from(tab_count);
    for (item, index) in tab_items.iter().zip(0..tab_count) {
        tabs.push(Tab {
            bounds: rect(
                bounds.origin.x + tab_width * f32::from(index),
                bounds.origin.y,
                tab_width,
                tab_height,
            ),
            item: item.clone(),
            active: active.as_ref() == Some(item),
        });
    }
    if let Some(active) = active {
        project_item(
            &active,
            rect(
                bounds.origin.x,
                bounds.origin.y + tab_height,
                bounds.size.width(),
                (bounds.size.height() - tab_height).max(0.0),
            ),
            items,
            item_bounds,
            missing_items,
        );
    }
}

fn first_panel(node: &LayoutNode) -> Option<PanelId> {
    match node {
        LayoutNode::Split { first, second, .. } => {
            first_panel(first).or_else(|| first_panel(second))
        }
        LayoutNode::Tabs { items, .. } => items.first().cloned(),
        LayoutNode::Panel { id } => Some(id.clone()),
        LayoutNode::Documents { .. } | LayoutNode::Empty => None,
    }
}

fn split_bounds(bounds: Rect, axis: DockAxis, ratio: f32) -> (Rect, Rect, Rect) {
    let length = match axis {
        DockAxis::Horizontal => bounds.size.width(),
        DockAxis::Vertical => bounds.size.height(),
    };
    let splitter_length = length.min(SPLITTER_THICKNESS);
    let content_length = (length - splitter_length).max(0.0);
    let first_length = content_length * ratio;
    let second_length = content_length - first_length;
    match axis {
        DockAxis::Horizontal => {
            let first = rect(
                bounds.origin.x,
                bounds.origin.y,
                first_length,
                bounds.size.height(),
            );
            let splitter = rect(
                bounds.origin.x + first_length,
                bounds.origin.y,
                splitter_length,
                bounds.size.height(),
            );
            let second = rect(
                splitter.right(),
                bounds.origin.y,
                second_length,
                bounds.size.height(),
            );
            (first, splitter, second)
        }
        DockAxis::Vertical => {
            let first = rect(
                bounds.origin.x,
                bounds.origin.y,
                bounds.size.width(),
                first_length,
            );
            let splitter = rect(
                bounds.origin.x,
                bounds.origin.y + first_length,
                bounds.size.width(),
                splitter_length,
            );
            let second = rect(
                bounds.origin.x,
                splitter.bottom(),
                bounds.size.width(),
                second_length,
            );
            (first, splitter, second)
        }
    }
}

fn project_item(
    item: &DockItem,
    bounds: Rect,
    items: &[DockItem],
    item_bounds: &mut [Option<Rect>],
    missing_items: &mut Vec<Rect>,
) {
    if let Some(index) = items.iter().position(|candidate| candidate == item) {
        item_bounds[index] = Some(bounds);
    } else {
        missing_items.push(bounds);
    }
}

fn offset_rect(rect: Rect, offset: Point) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + offset.x, rect.origin.y + offset.y),
        rect.size,
    )
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(
        Point::new(x, y),
        Size::new(width, height).expect("projected dock rectangle extents are non-negative"),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use torn_core::{
        Color, Constraints, InputEvent, Modifiers, Point, PointerButton, PointerButtons,
        PointerEvent, PointerId, Rect, Size,
    };
    use torn_render::{DisplayCommand, DisplayList, PaintContext};
    use torn_ui::{
        EventContext, EventStatus, LayoutContext, LayoutResult, LightTheme, Theme, UiRuntime,
        Widget,
    };
    use torn_workspace::{DockAxis, LayoutNode, PanelId, WorkspaceLayout};

    use super::DockArea;

    struct Fill(Color);

    impl Widget for Fill {
        fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
            LayoutResult::new(constraints.constrain(Size::ZERO))
        }

        fn paint(&self, context: &mut PaintContext<'_>, _: &torn_ui::UiEnvironment, bounds: Rect) {
            context.fill_rect(bounds, self.0);
        }
    }

    struct EventFill(Rc<Cell<u32>>);

    impl Widget for EventFill {
        fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
            LayoutResult::new(constraints.constrain(Size::ZERO))
        }

        fn handle_event(&mut self, _: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
            if matches!(event, InputEvent::PointerDown(_)) {
                self.0.set(self.0.get() + 1);
                EventStatus::Handled
            } else {
                EventStatus::Ignored
            }
        }
    }

    fn panel(id: &str) -> PanelId {
        PanelId::from(id)
    }

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn projects_splits_and_only_the_active_tab() {
        let left = panel("left");
        let first_tab = panel("first-tab");
        let active_tab = panel("active-tab");
        let layout = Rc::new(RefCell::new(
            WorkspaceLayout::new(LayoutNode::split(
                DockAxis::Horizontal,
                0.25,
                LayoutNode::Panel { id: left.clone() },
                LayoutNode::Tabs {
                    items: vec![first_tab.clone(), active_tab.clone()],
                    active: Some(active_tab.clone()),
                },
            ))
            .expect("valid workspace"),
        ));
        let mut dock_area = DockArea::new(layout);
        dock_area.register_panel(left).expect("unique binding");
        dock_area.register_panel(first_tab).expect("unique binding");
        dock_area
            .register_panel(active_tab)
            .expect("unique binding");
        let mut runtime = UiRuntime::new(dock_area);
        let root = runtime.root();
        let left_widget = runtime
            .append_child(root, Fill(Color::rgba8(10, 20, 30, 255)))
            .expect("root exists");
        let inactive_widget = runtime
            .append_child(root, Fill(Color::rgba8(40, 50, 60, 255)))
            .expect("root exists");
        let active_widget = runtime
            .append_child(root, Fill(Color::rgba8(70, 80, 90, 255)))
            .expect("root exists");

        runtime
            .layout(Constraints::tight(size(100.0, 60.0)).expect("tight canvas"))
            .expect("dock layout succeeds");

        assert_eq!(
            runtime.bounds(left_widget),
            Some(Rect::new(Point::ZERO, size(24.0, 60.0)))
        );
        assert_eq!(runtime.bounds(inactive_widget), Some(Rect::ZERO));
        assert_eq!(
            runtime.bounds(active_widget),
            Some(Rect::new(Point::new(28.0, 30.0), size(72.0, 30.0)))
        );

        let mut list = DisplayList::new();
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("dock paint succeeds");
        assert!(matches!(
            list.commands(),
            [
                DisplayCommand::FillRect { color, .. },
                DisplayCommand::FillRect { color: splitter, .. },
                DisplayCommand::FillRect { color: first_tab, .. },
                DisplayCommand::DrawText { .. },
                DisplayCommand::FillRect { color: active_tab, .. },
                DisplayCommand::DrawText { .. },
                DisplayCommand::FillRect { color: left, .. },
                DisplayCommand::FillRect { color: active, .. },
            ] if *color == LightTheme.background()
                && *splitter == LightTheme.button_background()
                && *first_tab == LightTheme.button_background()
                && *active_tab == LightTheme.accent()
                && *left == Color::rgba8(10, 20, 30, 255)
                && *active == Color::rgba8(70, 80, 90, 255)
        ));
    }

    #[test]
    fn shared_layout_reprojects_after_the_active_panel_changes() {
        let first = panel("first");
        let second = panel("second");
        let layout = Rc::new(RefCell::new(
            WorkspaceLayout::new(LayoutNode::tabs(vec![first.clone(), second.clone()]))
                .expect("valid workspace"),
        ));
        let mut dock_area = DockArea::new(layout.clone());
        dock_area
            .register_panel(first.clone())
            .expect("unique binding");
        dock_area
            .register_panel(second.clone())
            .expect("unique binding");
        let mut runtime = UiRuntime::new(dock_area);
        let root = runtime.root();
        let first_widget = runtime
            .append_child(root, Fill(Color::BLACK))
            .expect("root exists");
        let second_widget = runtime
            .append_child(root, Fill(Color::WHITE))
            .expect("root exists");
        let canvas = Constraints::tight(size(80.0, 40.0)).expect("tight canvas");

        runtime.layout(canvas).expect("initial layout succeeds");
        assert_eq!(
            runtime.bounds(first_widget),
            Some(Rect::new(Point::new(0.0, 30.0), size(80.0, 10.0)))
        );
        assert_eq!(runtime.bounds(second_widget), Some(Rect::ZERO));

        layout
            .borrow_mut()
            .activate_panel(&second)
            .expect("second panel is docked");
        runtime.layout(canvas).expect("updated layout succeeds");

        assert_eq!(runtime.bounds(first_widget), Some(Rect::ZERO));
        assert_eq!(
            runtime.bounds(second_widget),
            Some(Rect::new(Point::new(0.0, 30.0), size(80.0, 10.0)))
        );
    }

    #[test]
    fn inactive_tabs_are_excluded_from_hit_testing() {
        let first = panel("first");
        let second = panel("second");
        let layout = Rc::new(RefCell::new(
            WorkspaceLayout::new(LayoutNode::tabs(vec![first.clone(), second.clone()]))
                .expect("valid workspace"),
        ));
        let mut dock_area = DockArea::new(layout.clone());
        dock_area.register_panel(first).expect("unique binding");
        dock_area
            .register_panel(second.clone())
            .expect("unique binding");
        let mut runtime = UiRuntime::new(dock_area);
        let first_events = Rc::new(Cell::new(0));
        let second_events = Rc::new(Cell::new(0));
        let root = runtime.root();
        runtime
            .append_child(root, EventFill(first_events.clone()))
            .expect("root exists");
        runtime
            .append_child(root, EventFill(second_events.clone()))
            .expect("root exists");
        let canvas = Constraints::tight(size(80.0, 40.0)).expect("tight canvas");
        let pointer_down = InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(20.0, 35.0),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });

        runtime.layout(canvas).expect("initial layout succeeds");
        assert!(runtime.dispatch_event(&pointer_down).is_handled());
        assert_eq!(first_events.get(), 1);
        assert_eq!(second_events.get(), 0);

        layout
            .borrow_mut()
            .activate_panel(&second)
            .expect("second panel is docked");
        runtime.layout(canvas).expect("updated layout succeeds");
        assert!(runtime.dispatch_event(&pointer_down).is_handled());
        assert_eq!(first_events.get(), 1);
        assert_eq!(second_events.get(), 1);
    }

    #[test]
    fn clicking_a_tab_activates_its_registered_panel() {
        let first = panel("first");
        let second = panel("second");
        let layout = Rc::new(RefCell::new(
            WorkspaceLayout::new(LayoutNode::tabs(vec![first.clone(), second.clone()]))
                .expect("valid workspace"),
        ));
        let mut dock_area = DockArea::new(layout.clone());
        dock_area.register_panel(first).expect("unique binding");
        dock_area
            .register_panel(second.clone())
            .expect("unique binding");
        let mut runtime = UiRuntime::new(dock_area);
        let root = runtime.root();
        runtime
            .append_child(root, Fill(Color::BLACK))
            .expect("root exists");
        runtime
            .append_child(root, Fill(Color::WHITE))
            .expect("root exists");
        runtime
            .layout(Constraints::tight(size(100.0, 80.0)).expect("tight canvas"))
            .expect("initial layout succeeds");
        let click_second_tab = InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(75.0, 10.0),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });

        assert!(runtime.dispatch_event(&click_second_tab).is_handled());
        assert!(matches!(
            layout.borrow().root(),
            LayoutNode::Tabs { active: Some(active), .. } if active == &second
        ));
    }

    #[test]
    fn dragging_a_splitter_updates_the_persisted_ratio() {
        let left = panel("left");
        let right = panel("right");
        let layout = Rc::new(RefCell::new(
            WorkspaceLayout::new(LayoutNode::split(
                DockAxis::Horizontal,
                0.5,
                LayoutNode::Panel { id: left.clone() },
                LayoutNode::Panel { id: right.clone() },
            ))
            .expect("valid workspace"),
        ));
        let mut dock_area = DockArea::new(layout.clone());
        dock_area.register_panel(left).expect("unique binding");
        dock_area.register_panel(right).expect("unique binding");
        let mut runtime = UiRuntime::new(dock_area);
        let root = runtime.root();
        runtime
            .append_child(root, Fill(Color::BLACK))
            .expect("root exists");
        runtime
            .append_child(root, Fill(Color::WHITE))
            .expect("root exists");
        runtime
            .layout(Constraints::tight(size(100.0, 80.0)).expect("tight canvas"))
            .expect("initial layout succeeds");
        let down = InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(49.0, 20.0),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });
        let move_right = InputEvent::PointerMove(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(86.0, 20.0),
            button: None,
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });

        assert!(runtime.dispatch_event(&down).is_handled());
        assert!(runtime.dispatch_event(&move_right).is_handled());
        assert!(matches!(
            layout.borrow().root(),
            LayoutNode::Split { ratio, .. } if (*ratio - (86.0 / 96.0)).abs() < f32::EPSILON
        ));
    }
}
