use std::{
    any::Any,
    collections::HashMap,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

use torn_core::{
    Constraints, Diagnostic, DiagnosticReporter, FocusChanged, InputEvent, Key, Modifiers,
    NamedKey, Point, PointerId, Rect, WidgetId,
};
use torn_render::PaintContext;

use crate::{
    EventContext, EventPhase, EventStatus, KeyboardCommand, LayoutResult, UiEnvironment, Widget,
    event,
};
use event::{EventEffects, FocusRequest};

const MAX_RETAINED_DIAGNOSTICS: usize = 256;

/// Per-node invalidation state maintained by [`UiRuntime`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirtyFlags {
    /// The node needs layout.
    pub layout: bool,
    /// The node needs repainting.
    pub paint: bool,
    /// The node's hit-test bounds have changed.
    pub hit_test: bool,
}

impl DirtyFlags {
    const ALL: Self = Self {
        layout: true,
        paint: true,
        hit_test: true,
    };
}

/// Context through which a widget measures its direct runtime-owned children.
pub struct LayoutContext<'a> {
    runtime: &'a mut UiRuntime,
    parent: WidgetId,
    parent_origin: Point,
}

impl LayoutContext<'_> {
    /// Returns the runtime-wide environment for the current layout pass.
    #[must_use]
    pub fn environment(&self) -> &UiEnvironment {
        &self.runtime.environment
    }

    /// Returns the number of direct children owned by the runtime.
    #[must_use]
    pub fn child_count(&self) -> usize {
        self.runtime
            .node(self.parent)
            .map_or(0, |node| node.children.len())
    }

    /// Measures the child at `index` and returns its stable handle and result.
    ///
    /// The widget must include each direct child once in its [`LayoutResult`].
    ///
    /// # Errors
    ///
    /// Returns [`UiRuntimeError::InvalidWidgetId`] when `index` does not name a
    /// direct child, or when a child handle became invalid during layout.
    pub fn layout_child(
        &mut self,
        index: usize,
        constraints: Constraints,
    ) -> Result<(WidgetId, LayoutResult), UiRuntimeError> {
        let id = *self
            .runtime
            .node(self.parent)
            .and_then(|node| node.children.get(index))
            .ok_or(UiRuntimeError::InvalidWidgetId)?;
        let result = self
            .runtime
            .layout_node(id, constraints, self.parent_origin)?;
        Ok((id, result))
    }
}

/// Retained UI runtime responsible for tree ownership, layout, painting, input routing, and focus.
///
/// Every widget occupies a generational arena slot. Nodes own their parent and
/// child relationships, absolute layout bounds, invalidation state, and routing
/// metadata; widgets themselves only implement behavior.
pub struct UiRuntime {
    nodes: Vec<ArenaSlot>,
    free_slots: Vec<u32>,
    root: WidgetId,
    layout: Option<LayoutResult>,
    last_constraints: Option<Constraints>,
    diagnostics: Vec<Diagnostic>,
    reporter: Option<Box<dyn DiagnosticReporter>>,
    pointer_capture: HashMap<PointerId, WidgetId>,
    hovered: HashMap<PointerId, WidgetId>,
    focused: Option<WidgetId>,
    commands: Vec<KeyboardCommand>,
    redraw_requested: bool,
    environment: UiEnvironment,
}

struct ArenaSlot {
    generation: u32,
    node: Option<Node>,
}

struct Node {
    widget: Option<Box<dyn Widget>>,
    parent: Option<WidgetId>,
    children: Vec<WidgetId>,
    bounds: Rect,
    visible: bool,
    dirty: DirtyFlags,
    accepts_focus: bool,
}

/// Why a [`UiRuntime`] operation could not be completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiRuntimeError {
    /// Application widget code panicked during the requested operation.
    WidgetPanicked,
    /// A widget supplied child layouts that do not match its retained children.
    InvalidLayout,
    /// A requested node handle is stale or does not belong to this runtime.
    InvalidWidgetId,
}

impl fmt::Display for UiRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WidgetPanicked => "application widget panicked; inspect runtime diagnostics",
            Self::InvalidLayout => {
                "widget layout must position distinct direct children in tree order"
            }
            Self::InvalidWidgetId => "widget handle is stale or does not belong to this runtime",
        })
    }
}

impl std::error::Error for UiRuntimeError {}

impl UiRuntime {
    /// Creates a runtime whose arena root is `root`.
    #[must_use]
    pub fn new(root: impl Widget + 'static) -> Self {
        Self::with_environment(root, UiEnvironment::default())
    }

    /// Creates a runtime whose arena root is `root` and which owns `environment`.
    #[must_use]
    pub fn with_environment(root: impl Widget + 'static, environment: UiEnvironment) -> Self {
        let mut runtime = Self {
            nodes: Vec::new(),
            free_slots: Vec::new(),
            root: WidgetId::new(0, 0),
            layout: None,
            last_constraints: None,
            diagnostics: Vec::new(),
            reporter: None,
            pointer_capture: HashMap::new(),
            hovered: HashMap::new(),
            focused: None,
            commands: Vec::new(),
            redraw_requested: true,
            environment,
        };
        runtime.root = runtime.insert_node(Box::new(root), None);
        runtime
    }

    /// Returns the environment shared by all widgets in this runtime.
    #[must_use]
    pub const fn environment(&self) -> &UiEnvironment {
        &self.environment
    }

    /// Returns mutable access to the shared environment and invalidates layout.
    ///
    /// Theme and scale changes can affect a widget's intrinsic size, so the
    /// retained layout is cleared before the environment is exposed.
    pub fn environment_mut(&mut self) -> &mut UiEnvironment {
        self.invalidate_layout();
        &mut self.environment
    }

    /// Returns the stable handle of the root node.
    #[must_use]
    pub const fn root(&self) -> WidgetId {
        self.root
    }

    /// Appends `child` to `parent` and returns its stable generational handle.
    ///
    /// # Errors
    ///
    /// Returns [`UiRuntimeError::InvalidWidgetId`] when `parent` is stale.
    pub fn append_child(
        &mut self,
        parent: WidgetId,
        child: impl Widget + 'static,
    ) -> Result<WidgetId, UiRuntimeError> {
        if self.node(parent).is_none() {
            return Err(UiRuntimeError::InvalidWidgetId);
        }
        let id = self.insert_node(Box::new(child), Some(parent));
        self.node_mut(parent)
            .ok_or(UiRuntimeError::InvalidWidgetId)?
            .children
            .push(id);
        self.invalidate_layout();
        Ok(id)
    }

    /// Removes `node` and all of its descendants, invalidating their handles.
    ///
    /// The root cannot be removed.
    ///
    /// # Errors
    ///
    /// Returns [`UiRuntimeError::InvalidWidgetId`] when `node` is stale or is
    /// the runtime root.
    pub fn remove_subtree(&mut self, node: WidgetId) -> Result<(), UiRuntimeError> {
        if node == self.root {
            return Err(UiRuntimeError::InvalidWidgetId);
        }
        if self.node(node).is_none() {
            return Err(UiRuntimeError::InvalidWidgetId);
        }
        let parent = self
            .node(node)
            .and_then(|node| node.parent)
            .ok_or(UiRuntimeError::InvalidWidgetId)?;
        self.node_mut(parent)
            .ok_or(UiRuntimeError::InvalidWidgetId)?
            .children
            .retain(|id| *id != node);
        self.deallocate_subtree(node);
        self.invalidate_layout();
        Ok(())
    }

    /// Removes `node` and all of its descendants.
    ///
    /// This is an alias for [`Self::remove_subtree`]. Prefer that method when
    /// the fact that descendants are removed matters to the calling code.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::remove_subtree`].
    pub fn remove(&mut self, node: WidgetId) -> Result<(), UiRuntimeError> {
        self.remove_subtree(node)
    }

    /// Returns a node's parent, if its handle is live.
    #[must_use]
    pub fn parent(&self, node: WidgetId) -> Option<WidgetId> {
        self.node(node).and_then(|node| node.parent)
    }

    /// Returns direct child handles in display order, if `node` is live.
    #[must_use]
    pub fn children(&self, node: WidgetId) -> Option<&[WidgetId]> {
        self.node(node).map(|node| node.children.as_slice())
    }

    /// Returns the last absolute bounds assigned to a live node.
    #[must_use]
    pub fn bounds(&self, node: WidgetId) -> Option<Rect> {
        self.node(node).map(|node| node.bounds)
    }

    /// Returns the invalidation state of a live node.
    #[must_use]
    pub fn dirty_flags(&self, node: WidgetId) -> Option<DirtyFlags> {
        self.node(node).map(|node| node.dirty)
    }

    /// Returns mutable access to a widget and invalidates the retained layout.
    pub fn widget_mut(&mut self, node: WidgetId) -> Option<&mut (dyn Widget + 'static)> {
        self.invalidate_layout();
        self.node_mut(node)?.widget.as_deref_mut()
    }

    /// Returns mutable access to the root widget and invalidates the retained layout.
    ///
    /// # Panics
    ///
    /// Panics only if the runtime's internally retained root node was corrupted.
    pub fn root_mut(&mut self) -> &mut (dyn Widget + 'static) {
        self.widget_mut(self.root)
            .expect("runtime root is always live")
    }

    /// Sets a reporter that receives every runtime diagnostic.
    pub fn set_diagnostic_reporter(&mut self, reporter: impl DiagnosticReporter + 'static) {
        self.reporter = Some(Box::new(reporter));
    }

    /// Removes the external diagnostic reporter, if one is configured.
    pub fn clear_diagnostic_reporter(&mut self) {
        self.reporter = None;
    }

    /// Returns up to the 256 most recent diagnostics emitted by the runtime.
    ///
    /// Older diagnostics are discarded so a repeatedly failing application widget
    /// cannot make the runtime retain unbounded error messages. An external
    /// [`DiagnosticReporter`] still receives every emitted diagnostic.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Drains and returns diagnostics emitted by the runtime.
    #[must_use]
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Lays out the retained tree and returns the computed root layout.
    ///
    /// # Errors
    ///
    /// Returns [`UiRuntimeError::WidgetPanicked`] when widget code panics, or
    /// [`UiRuntimeError::InvalidLayout`] when a container positions an unknown,
    /// duplicate, or out-of-order child.
    pub fn layout(&mut self, constraints: Constraints) -> Result<&LayoutResult, UiRuntimeError> {
        match catch_unwind(AssertUnwindSafe(|| {
            self.layout_node(self.root, constraints, Point::ZERO)
        })) {
            Ok(Ok(layout)) => {
                self.layout = Some(layout);
                self.last_constraints = Some(constraints);
                self.redraw_requested = true;
                self.layout.as_ref().ok_or(UiRuntimeError::InvalidLayout)
            }
            Ok(Err(error)) => {
                self.clear_layout_state();
                Err(error)
            }
            Err(payload) => {
                self.clear_layout_state();
                self.report_panic("layout", payload);
                Err(UiRuntimeError::WidgetPanicked)
            }
        }
    }

    /// Returns the most recently computed root layout, if any.
    #[must_use]
    pub const fn last_layout(&self) -> Option<&LayoutResult> {
        self.layout.as_ref()
    }

    /// Returns the widget currently receiving keyboard input.
    #[must_use]
    pub const fn focused_widget(&self) -> Option<WidgetId> {
        self.focused
    }

    /// Returns the widget currently capturing `pointer_id`, if any.
    #[must_use]
    pub fn pointer_capture(&self, pointer_id: PointerId) -> Option<WidgetId> {
        self.pointer_capture.get(&pointer_id).copied()
    }

    /// Registers an application keyboard command.
    pub fn register_command(&mut self, command: KeyboardCommand) {
        self.commands.push(command);
    }

    /// Returns whether a handler requested another frame since the last call.
    pub fn take_redraw_request(&mut self) -> bool {
        std::mem::replace(&mut self.redraw_requested, false)
    }

    /// Records paint operations for the retained tree.
    ///
    /// # Errors
    ///
    /// Returns [`UiRuntimeError::WidgetPanicked`] when widget paint code panics.
    pub fn paint(&mut self, context: &mut PaintContext<'_>) -> Result<(), UiRuntimeError> {
        if self.layout.is_none()
            && let Some(constraints) = self.last_constraints
        {
            self.layout(constraints)?;
        }
        if self.layout.is_none() {
            return Ok(());
        }
        match catch_unwind(AssertUnwindSafe(|| self.paint_node(self.root, context))) {
            Ok(()) => Ok(()),
            Err(payload) => {
                self.report_panic("paint", payload);
                Err(UiRuntimeError::WidgetPanicked)
            }
        }
    }

    /// Delivers an input event through capture, target, and bubble phases.
    pub fn dispatch_event(&mut self, event: &InputEvent) -> EventStatus {
        if let InputEvent::KeyDown(key) = event {
            if self.dispatch_tab_navigation(key) || self.dispatch_command(key) {
                return EventStatus::Handled;
            }
        }
        self.update_hover(event);
        let Some(target) = self.select_target(event) else {
            return EventStatus::Ignored;
        };
        let route = self.route_to(target);
        let mut effects = EventEffects {
            propagation_stopped: false,
            pointer_capture: self.pointer_capture.clone(),
            focused: self.focused,
            focus_request: None,
            layout_requested: false,
            redraw_requested: false,
        };
        let mut handled = EventStatus::Ignored;

        for id in route.iter().take(route.len().saturating_sub(1)) {
            handled = merge_status(
                handled,
                self.dispatch_to(*id, target, EventPhase::Capture, event, &mut effects),
            );
            if effects.propagation_stopped {
                self.finish_event(event, effects);
                return handled;
            }
        }
        handled = merge_status(
            handled,
            self.dispatch_to(target, target, EventPhase::Target, event, &mut effects),
        );
        if !effects.propagation_stopped {
            for id in route.iter().rev().skip(1) {
                handled = merge_status(
                    handled,
                    self.dispatch_to(*id, target, EventPhase::Bubble, event, &mut effects),
                );
                if effects.propagation_stopped {
                    break;
                }
            }
        }
        self.finish_event(event, effects);
        handled
    }

    fn layout_node(
        &mut self,
        id: WidgetId,
        constraints: Constraints,
        origin: Point,
    ) -> Result<LayoutResult, UiRuntimeError> {
        let mut widget = self
            .node_mut(id)
            .ok_or(UiRuntimeError::InvalidWidgetId)?
            .widget
            .take()
            .expect("widget is present outside layout call");
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut context = LayoutContext {
                runtime: self,
                parent: id,
                parent_origin: origin,
            };
            widget.layout(&mut context, constraints)
        }));
        self.node_mut(id).expect("node survives layout").widget = Some(widget);
        let result = match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        };

        self.apply_layout(id, origin, &result)?;
        Ok(result)
    }

    fn apply_layout(
        &mut self,
        id: WidgetId,
        origin: Point,
        result: &LayoutResult,
    ) -> Result<(), UiRuntimeError> {
        let children = self
            .node(id)
            .ok_or(UiRuntimeError::InvalidWidgetId)?
            .children
            .clone();
        let mut next_child = 0;
        for layout in result.children() {
            let Some(index) = children[next_child..]
                .iter()
                .position(|child| *child == layout.id())
            else {
                return Err(UiRuntimeError::InvalidLayout);
            };
            next_child += index + 1;
        }

        let accepts_focus = self
            .node(id)
            .expect("node was validated")
            .widget
            .as_ref()
            .is_some_and(Widget::accepts_focus);
        let node = self.node_mut(id).expect("node was validated");
        node.bounds = Rect::new(origin, result.size());
        node.dirty = DirtyFlags::default();
        node.accepts_focus = accepts_focus;
        for child in &children {
            self.node_mut(*child).expect("child was validated").visible = false;
        }
        for child in result.children() {
            self.node_mut(child.id())
                .expect("child was validated")
                .visible = child.is_visible();
            self.translate_subtree(
                child.id(),
                Point::new(origin.x + child.origin().x, origin.y + child.origin().y),
            );
        }
        Ok(())
    }

    fn translate_subtree(&mut self, id: WidgetId, origin: Point) {
        let Some(node) = self.node(id) else {
            return;
        };
        let old = node.bounds.origin;
        let children = node.children.clone();
        let delta = Point::new(origin.x - old.x, origin.y - old.y);
        self.node_mut(id).expect("node is live").bounds.origin = origin;
        for child in children {
            let child_origin = self.node(child).expect("child is live").bounds.origin;
            self.translate_subtree(
                child,
                Point::new(child_origin.x + delta.x, child_origin.y + delta.y),
            );
        }
    }

    fn paint_node(&self, id: WidgetId, context: &mut PaintContext<'_>) {
        let node = self.node(id).expect("paint tree contains live nodes");
        if !node.visible {
            return;
        }
        node.widget
            .as_ref()
            .expect("widget is present outside layout call")
            .paint(context, &self.environment, node.bounds);
        let clips_children = node
            .widget
            .as_ref()
            .expect("widget is present outside layout call")
            .clips_children();
        if clips_children {
            context.with_clip(node.bounds, |context| {
                for child in &node.children {
                    self.paint_node(*child, context);
                }
            });
        } else {
            for child in &node.children {
                self.paint_node(*child, context);
            }
        }
    }

    fn select_target(&self, event: &InputEvent) -> Option<WidgetId> {
        if let Some(pointer_id) = pointer_id(event) {
            return self
                .pointer_capture
                .get(&pointer_id)
                .copied()
                .filter(|id| self.is_visible(*id))
                .or_else(|| {
                    event::pointer_position(event).and_then(|position| self.hit_test(position))
                });
        }
        if matches!(event, InputEvent::Wheel(_)) {
            return event::pointer_position(event).and_then(|position| self.hit_test(position));
        }
        self.focused.filter(|id| self.is_visible(*id))
    }

    fn update_hover(&mut self, event: &InputEvent) {
        let InputEvent::PointerMove(pointer) = event else {
            return;
        };
        let next = self.hit_test(pointer.position);
        let previous = self.hovered.get(&pointer.pointer_id).copied();
        if previous == next {
            return;
        }
        if let Some(previous) = previous.filter(|id| self.is_visible(*id)) {
            self.dispatch_synthetic_pointer(previous, &InputEvent::PointerLeave(pointer.clone()));
        }
        if let Some(next) = next {
            self.hovered.insert(pointer.pointer_id, next);
            self.dispatch_synthetic_pointer(next, &InputEvent::PointerEnter(pointer.clone()));
        } else {
            self.hovered.remove(&pointer.pointer_id);
        }
    }

    fn dispatch_synthetic_pointer(&mut self, target: WidgetId, event: &InputEvent) {
        let mut effects = EventEffects {
            propagation_stopped: false,
            pointer_capture: self.pointer_capture.clone(),
            focused: self.focused,
            focus_request: None,
            layout_requested: false,
            redraw_requested: false,
        };
        let route = self.route_to(target);
        for id in route.iter().take(route.len().saturating_sub(1)) {
            let _ = self.dispatch_to(*id, target, EventPhase::Capture, event, &mut effects);
            if effects.propagation_stopped {
                self.finish_event(event, effects);
                return;
            }
        }
        let _ = self.dispatch_to(target, target, EventPhase::Target, event, &mut effects);
        if !effects.propagation_stopped {
            for id in route.iter().rev().skip(1) {
                let _ = self.dispatch_to(*id, target, EventPhase::Bubble, event, &mut effects);
                if effects.propagation_stopped {
                    break;
                }
            }
        }
        self.finish_event(event, effects);
    }

    fn dispatch_command(&mut self, key: &torn_core::KeyEvent) -> bool {
        let Some(command) = self
            .commands
            .iter()
            .find(|command| command.shortcut().matches(key))
        else {
            return false;
        };
        command.activate();
        self.redraw_requested = true;
        true
    }

    fn dispatch_tab_navigation(&mut self, key: &torn_core::KeyEvent) -> bool {
        if key.repeat || key.key != Key::Named(NamedKey::Tab) {
            return false;
        }
        if key.modifiers != Modifiers::NONE && key.modifiers != Modifiers::SHIFT {
            return false;
        }
        let focusable = self.focusable_nodes();
        if focusable.is_empty() {
            return false;
        }
        let current = self
            .focused
            .and_then(|focused| focusable.iter().position(|id| *id == focused));
        let index = match (current, key.modifiers == Modifiers::SHIFT) {
            (Some(index), false) => (index + 1) % focusable.len(),
            (Some(0), true) => focusable.len() - 1,
            (Some(index), true) => index - 1,
            (None, _) => 0,
        };
        self.set_focus(Some(focusable[index]));
        true
    }

    fn focusable_nodes(&self) -> Vec<WidgetId> {
        let mut result = Vec::new();
        self.collect_focusable(self.root, &mut result);
        result
    }

    fn collect_focusable(&self, id: WidgetId, result: &mut Vec<WidgetId>) {
        let Some(node) = self.node(id) else {
            return;
        };
        if !node.visible {
            return;
        }
        if node.accepts_focus {
            result.push(id);
        }
        for child in &node.children {
            self.collect_focusable(*child, result);
        }
    }

    fn hit_test(&self, position: Point) -> Option<WidgetId> {
        self.layout.as_ref()?;
        self.hit_test_node(self.root, position)
    }

    fn hit_test_node(&self, id: WidgetId, position: Point) -> Option<WidgetId> {
        let node = self.node(id)?;
        if !node.visible || !node.bounds.contains(position) {
            return None;
        }
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hit_test_node(*child, position) {
                return Some(hit);
            }
        }
        Some(id)
    }

    fn route_to(&self, target: WidgetId) -> Vec<WidgetId> {
        let mut route = Vec::new();
        let mut current = Some(target);
        while let Some(id) = current {
            route.push(id);
            current = self.node(id).and_then(|node| node.parent);
        }
        route.reverse();
        route
    }

    fn is_visible(&self, id: WidgetId) -> bool {
        let mut current = Some(id);
        while let Some(node_id) = current {
            let Some(node) = self.node(node_id) else {
                return false;
            };
            if !node.visible {
                return false;
            }
            current = node.parent;
        }
        true
    }

    fn dispatch_to(
        &mut self,
        id: WidgetId,
        target: WidgetId,
        phase: EventPhase,
        event: &InputEvent,
        effects: &mut EventEffects,
    ) -> EventStatus {
        let origin = self
            .node(id)
            .expect("route contains live nodes")
            .bounds
            .origin;
        let local_event = event::with_local_position(event, origin);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let widget = self
                .node_mut(id)
                .expect("route contains live nodes")
                .widget
                .as_deref_mut()
                .expect("widget is present outside layout call");
            let mut context = EventContext::new(phase, target, id, effects);
            widget.handle_event(&mut context, &local_event)
        }));
        match result {
            Ok(status) => status,
            Err(payload) => {
                effects.propagation_stopped = true;
                self.report_panic("event handler", payload);
                EventStatus::Ignored
            }
        }
    }

    fn finish_event(&mut self, event: &InputEvent, mut effects: EventEffects) {
        if let Some(pointer_id) = pointer_id(event) {
            if matches!(event, InputEvent::PointerUp(_)) {
                effects.pointer_capture.remove(&pointer_id);
            }
        }
        self.pointer_capture = effects.pointer_capture;
        self.redraw_requested |= effects.redraw_requested;
        if effects.layout_requested {
            self.invalidate_layout();
        }
        let Some(request) = effects.focus_request else {
            return;
        };
        let requested = match request {
            FocusRequest::Set(id) => self
                .node(id)
                .is_some_and(|node| node.accepts_focus)
                .then_some(id),
            FocusRequest::Clear => None,
        };
        self.set_focus(requested);
    }

    fn set_focus(&mut self, requested: Option<WidgetId>) {
        if requested == self.focused {
            return;
        }
        let previous = self.focused;
        self.focused = requested;
        self.redraw_requested = true;
        for target in previous.into_iter().chain(requested) {
            let mut focus_effects = EventEffects {
                propagation_stopped: false,
                pointer_capture: self.pointer_capture.clone(),
                focused: self.focused,
                focus_request: None,
                layout_requested: false,
                redraw_requested: false,
            };
            let _ = self.dispatch_to(
                target,
                target,
                EventPhase::Target,
                &InputEvent::FocusChanged(FocusChanged { focused: requested }),
                &mut focus_effects,
            );
            self.pointer_capture = focus_effects.pointer_capture;
            self.redraw_requested |= focus_effects.redraw_requested;
        }
    }

    fn insert_node(&mut self, widget: Box<dyn Widget>, parent: Option<WidgetId>) -> WidgetId {
        let node = Node {
            widget: Some(widget),
            parent,
            children: Vec::new(),
            bounds: Rect::ZERO,
            visible: true,
            dirty: DirtyFlags::ALL,
            accepts_focus: false,
        };
        if let Some(index) = self.free_slots.pop() {
            let slot = &mut self.nodes[usize::try_from(index).expect("u32 fits usize")];
            slot.node = Some(node);
            return WidgetId::new(index, slot.generation);
        }
        let index =
            u32::try_from(self.nodes.len()).expect("widget arena exceeds WidgetId capacity");
        self.nodes.push(ArenaSlot {
            generation: 0,
            node: Some(node),
        });
        WidgetId::new(index, 0)
    }

    fn deallocate_subtree(&mut self, id: WidgetId) {
        let children = self.node(id).expect("node is live").children.clone();
        for child in children {
            self.deallocate_subtree(child);
        }
        let index = usize::try_from(id.index()).expect("u32 fits usize");
        let slot = &mut self.nodes[index];
        slot.node = None;
        if let Some(next_generation) = slot.generation.checked_add(1) {
            slot.generation = next_generation;
            self.free_slots.push(id.index());
        }
        self.pointer_capture.retain(|_, captured| *captured != id);
        self.hovered.retain(|_, hovered| *hovered != id);
        if self.focused == Some(id) {
            self.focused = None;
        }
    }

    fn node(&self, id: WidgetId) -> Option<&Node> {
        let index = usize::try_from(id.index()).ok()?;
        let slot = self.nodes.get(index)?;
        (slot.generation == id.generation()).then_some(slot.node.as_ref()?)
    }

    fn node_mut(&mut self, id: WidgetId) -> Option<&mut Node> {
        let index = usize::try_from(id.index()).ok()?;
        let slot = self.nodes.get_mut(index)?;
        (slot.generation == id.generation()).then_some(slot.node.as_mut()?)
    }

    fn invalidate_layout(&mut self) {
        self.layout = None;
        self.redraw_requested = true;
        for slot in &mut self.nodes {
            if let Some(node) = &mut slot.node {
                node.dirty = DirtyFlags::ALL;
            }
        }
    }

    fn clear_layout_state(&mut self) {
        self.layout = None;
        self.pointer_capture.clear();
        self.hovered.clear();
        self.focused = None;
    }

    fn report_panic(&mut self, operation: &str, payload: Box<dyn Any + Send>) {
        let message = format!(
            "application widget panicked during {operation}: {}",
            panic_message(payload)
        );
        let diagnostic = Diagnostic::error("torn-ui", message);
        if let Some(reporter) = &mut self.reporter {
            reporter.report(&diagnostic);
        }
        if self.diagnostics.len() == MAX_RETAINED_DIAGNOSTICS {
            self.diagnostics.remove(0);
        }
        self.diagnostics.push(diagnostic);
    }
}

fn pointer_id(event: &InputEvent) -> Option<PointerId> {
    match event {
        InputEvent::PointerDown(event)
        | InputEvent::PointerMove(event)
        | InputEvent::PointerEnter(event)
        | InputEvent::PointerLeave(event)
        | InputEvent::PointerUp(event) => Some(event.pointer_id),
        InputEvent::Wheel(_)
        | InputEvent::KeyDown(_)
        | InputEvent::KeyUp(_)
        | InputEvent::TextInput(_)
        | InputEvent::FocusChanged(_) => None,
    }
}

fn merge_status(left: EventStatus, right: EventStatus) -> EventStatus {
    if left.is_handled() || right.is_handled() {
        EventStatus::Handled
    } else {
        EventStatus::Ignored
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_owned(),
            Err(_) => "non-string panic payload".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use torn_core::{
        Color, Constraints, InputEvent, Key, Modifiers, NamedKey, Point, PointerButton,
        PointerButtons, PointerEvent, PointerId, Rect, Size,
    };
    use torn_render::{DisplayList, PaintContext};

    use crate::{
        ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult,
        LightTheme, Row, Theme, UiEnvironment, UiRuntime, UiRuntimeError, Widget, event,
    };

    type EventRecord = (&'static str, EventPhase, Point);
    type Events = Rc<RefCell<Vec<EventRecord>>>;

    struct Recorder {
        size: Size,
        name: &'static str,
        events: Events,
        capture_on_down: bool,
        focus_on_down: bool,
    }

    struct EnvironmentRecorder {
        observed: Rc<RefCell<Vec<(Color, f32, String)>>>,
    }

    struct RepeatedlyPanickingWidget {
        panic_count: Rc<Cell<usize>>,
    }

    impl Widget for EnvironmentRecorder {
        fn layout(
            &mut self,
            context: &mut LayoutContext<'_>,
            constraints: Constraints,
        ) -> LayoutResult {
            let environment = context.environment();
            self.observed.borrow_mut().push((
                environment.theme().background(),
                environment.scale_factor(),
                environment.locale().to_owned(),
            ));
            LayoutResult::new(constraints.constrain(Size::ZERO))
        }

        fn paint(&self, _: &mut PaintContext<'_>, environment: &UiEnvironment, _: Rect) {
            self.observed.borrow_mut().push((
                environment.theme().background(),
                environment.scale_factor(),
                environment.locale().to_owned(),
            ));
        }
    }

    impl Widget for RepeatedlyPanickingWidget {
        fn layout(&mut self, _: &mut LayoutContext<'_>, _: Constraints) -> LayoutResult {
            self.panic_count.set(self.panic_count.get() + 1);
            panic!("repeated application failure")
        }
    }

    impl Widget for Recorder {
        fn layout(
            &mut self,
            context: &mut LayoutContext<'_>,
            constraints: Constraints,
        ) -> LayoutResult {
            let mut children = Vec::with_capacity(context.child_count());
            for index in 0..context.child_count() {
                let (child, _) = context
                    .layout_child(index, Constraints::loose(constraints.max()))
                    .expect("valid child");
                children.push(ChildLayout::new(child, Point::ZERO));
            }
            LayoutResult::with_children(constraints.constrain(self.size), children)
        }
        fn handle_event(
            &mut self,
            context: &mut EventContext<'_>,
            event: &InputEvent,
        ) -> EventStatus {
            if let Some(position) = event::pointer_position(event) {
                self.events
                    .borrow_mut()
                    .push((self.name, context.phase(), position));
            }
            if self.capture_on_down
                && matches!(event, InputEvent::PointerDown(_))
                && context.phase() == EventPhase::Target
            {
                if let InputEvent::PointerDown(pointer) = event {
                    context.capture_pointer(pointer.pointer_id);
                }
            }
            if self.focus_on_down
                && matches!(event, InputEvent::PointerDown(_))
                && context.phase() == EventPhase::Target
            {
                context.request_focus();
            }
            EventStatus::Handled
        }
        fn accepts_focus(&self) -> bool {
            self.focus_on_down
        }
    }

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }
    fn pointer(position: Point, down: bool) -> InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position,
            button: Some(PointerButton::Primary),
            buttons: if down {
                PointerButtons::PRIMARY
            } else {
                PointerButtons::NONE
            },
            modifiers: Modifiers::NONE,
        };
        if down {
            InputEvent::PointerDown(event)
        } else {
            InputEvent::PointerUp(event)
        }
    }

    #[test]
    fn adding_a_sibling_preserves_existing_widget_ids() {
        let mut runtime = UiRuntime::new(Row::new());
        let root = runtime.root();
        let first = runtime
            .append_child(
                root,
                Recorder {
                    size: size(1.0, 1.0),
                    name: "first",
                    events: Rc::new(RefCell::new(Vec::new())),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("valid parent");
        let second = runtime
            .append_child(
                root,
                Recorder {
                    size: size(1.0, 1.0),
                    name: "second",
                    events: Rc::new(RefCell::new(Vec::new())),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("valid parent");

        assert_eq!(runtime.parent(first), Some(root));
        assert_eq!(runtime.children(root), Some([first, second].as_slice()));
        assert_ne!(first, second);
    }

    #[test]
    fn removing_a_node_invalidates_its_generation() {
        let mut runtime = UiRuntime::new(Row::new());
        let root = runtime.root();
        let first = runtime
            .append_child(
                root,
                Recorder {
                    size: size(1.0, 1.0),
                    name: "first",
                    events: Rc::new(RefCell::new(Vec::new())),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("valid parent");
        runtime.remove_subtree(first).expect("remove child");
        let second = runtime
            .append_child(
                root,
                Recorder {
                    size: size(1.0, 1.0),
                    name: "second",
                    events: Rc::new(RefCell::new(Vec::new())),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("valid parent");

        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
        assert_ne!(first, second);
        assert!(runtime.bounds(first).is_none());
        assert_eq!(
            runtime.remove_subtree(first),
            Err(UiRuntimeError::InvalidWidgetId)
        );
    }

    #[test]
    fn removing_a_subtree_clears_focus_and_pointer_capture() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(Recorder {
            size: size(10.0, 10.0),
            name: "root",
            events: Rc::clone(&events),
            capture_on_down: false,
            focus_on_down: false,
        });
        let branch = runtime
            .append_child(
                runtime.root(),
                Recorder {
                    size: size(10.0, 10.0),
                    name: "branch",
                    events: Rc::clone(&events),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("add branch");
        let target = runtime
            .append_child(
                branch,
                Recorder {
                    size: size(10.0, 10.0),
                    name: "target",
                    events,
                    capture_on_down: true,
                    focus_on_down: true,
                },
            )
            .expect("add target");

        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");
        let _ = runtime.dispatch_event(&pointer(Point::new(5.0, 5.0), true));
        assert_eq!(runtime.focused_widget(), Some(target));
        assert_eq!(runtime.pointer_capture(PointerId(1)), Some(target));

        runtime
            .remove_subtree(branch)
            .expect("remove focused branch");

        assert_eq!(runtime.focused_widget(), None);
        assert_eq!(runtime.pointer_capture(PointerId(1)), None);
    }

    #[test]
    fn routes_events_through_runtime_owned_tree_with_local_coordinates() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(Recorder {
            size: size(20.0, 10.0),
            name: "root",
            events: Rc::clone(&events),
            capture_on_down: false,
            focus_on_down: false,
        });
        let parent = runtime
            .append_child(
                runtime.root(),
                Recorder {
                    size: size(20.0, 10.0),
                    name: "parent",
                    events: Rc::clone(&events),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("add parent");
        runtime
            .append_child(
                parent,
                Recorder {
                    size: size(20.0, 10.0),
                    name: "target",
                    events: Rc::clone(&events),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("add target");
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");
        assert_eq!(
            runtime.dispatch_event(&pointer(Point::new(15.0, 5.0), true)),
            EventStatus::Handled
        );
        assert_eq!(
            *events.borrow(),
            vec![
                ("root", EventPhase::Capture, Point::new(15.0, 5.0)),
                ("parent", EventPhase::Capture, Point::new(15.0, 5.0)),
                ("target", EventPhase::Target, Point::new(15.0, 5.0)),
                ("parent", EventPhase::Bubble, Point::new(15.0, 5.0)),
                ("root", EventPhase::Bubble, Point::new(15.0, 5.0)),
            ]
        );
    }

    #[test]
    fn routes_events_through_the_updated_tree_after_a_removal() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(Recorder {
            size: size(10.0, 10.0),
            name: "root",
            events: Rc::clone(&events),
            capture_on_down: false,
            focus_on_down: false,
        });
        let removed = runtime
            .append_child(
                runtime.root(),
                Recorder {
                    size: size(10.0, 10.0),
                    name: "removed",
                    events: Rc::clone(&events),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("add first child");
        runtime
            .append_child(
                runtime.root(),
                Recorder {
                    size: size(10.0, 10.0),
                    name: "remaining",
                    events: Rc::clone(&events),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("add second child");
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("initial layout succeeds");
        let _ = runtime.dispatch_event(&pointer(Point::new(5.0, 5.0), true));
        assert_eq!(events.borrow()[1].0, "remaining");
        events.borrow_mut().clear();

        runtime.remove_subtree(removed).expect("remove first child");
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("updated layout succeeds");
        assert_eq!(
            runtime.dispatch_event(&pointer(Point::new(5.0, 5.0), true)),
            EventStatus::Handled
        );
        assert_eq!(
            *events.borrow(),
            vec![
                ("root", EventPhase::Capture, Point::new(5.0, 5.0)),
                ("remaining", EventPhase::Target, Point::new(5.0, 5.0)),
                ("root", EventPhase::Bubble, Point::new(5.0, 5.0)),
            ]
        );
    }

    #[test]
    fn captured_pointer_routes_outside_original_bounds() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(Row::new());
        let root = runtime.root();
        runtime
            .append_child(
                root,
                Recorder {
                    size: size(10.0, 10.0),
                    name: "capturing",
                    events: Rc::clone(&events),
                    capture_on_down: true,
                    focus_on_down: false,
                },
            )
            .expect("add child");
        runtime
            .append_child(
                root,
                Recorder {
                    size: size(10.0, 10.0),
                    name: "other",
                    events: Rc::clone(&events),
                    capture_on_down: false,
                    focus_on_down: false,
                },
            )
            .expect("add child");
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");
        let _ = runtime.dispatch_event(&pointer(Point::new(5.0, 5.0), true));
        let _ = runtime.dispatch_event(&pointer(Point::new(15.0, 5.0), false));
        assert!(runtime.pointer_capture(PointerId(1)).is_none());
        assert_eq!(
            events.borrow().last().map(|event| event.0),
            Some("capturing")
        );
    }

    #[test]
    fn tab_and_shift_tab_cycle_focusable_widgets_in_tree_order() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(Row::new());
        let root = runtime.root();
        let first = runtime
            .append_child(
                root,
                Recorder {
                    size: size(10.0, 10.0),
                    name: "first",
                    events: Rc::clone(&events),
                    capture_on_down: false,
                    focus_on_down: true,
                },
            )
            .expect("add first focusable widget");
        let second = runtime
            .append_child(
                root,
                Recorder {
                    size: size(10.0, 10.0),
                    name: "second",
                    events,
                    capture_on_down: false,
                    focus_on_down: true,
                },
            )
            .expect("add second focusable widget");
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");
        let tab = |modifiers| {
            InputEvent::KeyDown(torn_core::KeyEvent {
                key: Key::Named(NamedKey::Tab),
                code: torn_core::KeyCode::Unidentified,
                repeat: false,
                modifiers,
            })
        };

        let _ = runtime.dispatch_event(&tab(Modifiers::NONE));
        assert_eq!(runtime.focused_widget(), Some(first));
        let _ = runtime.dispatch_event(&tab(Modifiers::NONE));
        assert_eq!(runtime.focused_widget(), Some(second));
        let _ = runtime.dispatch_event(&tab(Modifiers::SHIFT));
        assert_eq!(runtime.focused_widget(), Some(first));
    }

    #[test]
    fn exposes_the_same_environment_to_layout_and_paint() {
        let observed = Rc::new(RefCell::new(Vec::new()));
        let mut environment = UiEnvironment::new(LightTheme);
        environment.set_scale_factor(1.5);
        environment.set_locale("ru-RU");
        let expected = (
            LightTheme.background(),
            environment.scale_factor(),
            environment.locale().to_owned(),
        );
        let mut runtime = UiRuntime::with_environment(
            EnvironmentRecorder {
                observed: Rc::clone(&observed),
            },
            environment,
        );
        let mut display_list = DisplayList::new();

        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("paint succeeds");

        assert_eq!(*observed.borrow(), vec![expected.clone(), expected]);
    }

    #[test]
    fn retains_a_bounded_history_of_repeated_widget_panics() {
        let panic_count = Rc::new(Cell::new(0));
        let mut runtime = UiRuntime::new(RepeatedlyPanickingWidget {
            panic_count: Rc::clone(&panic_count),
        });

        for _ in 0..=super::MAX_RETAINED_DIAGNOSTICS {
            assert_eq!(
                runtime.layout(Constraints::UNBOUNDED),
                Err(UiRuntimeError::WidgetPanicked)
            );
        }

        assert_eq!(panic_count.get(), super::MAX_RETAINED_DIAGNOSTICS + 1);
        assert_eq!(runtime.diagnostics().len(), super::MAX_RETAINED_DIAGNOSTICS);
        assert_eq!(
            runtime.take_diagnostics().len(),
            super::MAX_RETAINED_DIAGNOSTICS
        );
    }
}
