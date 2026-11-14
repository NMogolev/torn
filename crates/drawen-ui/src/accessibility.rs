//! Semantic tree, accessibility roles, states, and platform bridge contracts.
//!
//! Drawen separates a widget's *behavior* from the accessibility vocabulary it
//! exposes. Widgets describe themselves through [`Accessibility`]; the runtime
//! composes those descriptions into a [`SemanticTree`] that carries stable
//! widget identities, absolute bounds, focus, and tab order. The same tree is
//! translated into an `accesskit` update, which platform adapters expose
//! through UI Automation on Windows, AT-SPI on Linux, and the macOS
//! Accessibility API.
//!
//! Building the semantics into the retained tree now means every future
//! control participates in the same contract. A custom widget overrides
//! [`crate::Widget::accessibility`] and optionally
//! [`crate::Widget::accessibility_action`]; it does not need to know which
//! native accessibility API is active.

use std::collections::HashMap;

use drawen_core::{Point, Rect, WidgetId};

/// The semantic role of a widget.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AccessibilityRole {
    /// No specific role; assistive technologies may ignore the node.
    #[default]
    GenericContainer,
    /// The root application window or document surface.
    Application,
    /// A top-level window.
    Window,
    /// A modal or modeless dialog.
    Dialog,
    /// A generic named region.
    Pane,
    /// A logical grouping of controls.
    Group,
    /// A toolbar.
    Toolbar,
    /// A menu bar.
    MenuBar,
    /// A menu or popup menu.
    Menu,
    /// A menu item.
    MenuItem,
    /// A button.
    Button,
    /// A checkbox.
    CheckBox,
    /// A radio button.
    RadioButton,
    /// A switch.
    Switch,
    /// A single-line text input.
    TextField,
    /// A multi-line text input.
    MultilineTextField,
    /// Static text.
    Label,
    /// An image.
    Image,
    /// A list container.
    List,
    /// A list box with selectable options.
    ListBox,
    /// A combo box.
    ComboBox,
    /// An item in a list.
    ListItem,
    /// A table or grid.
    Table,
    /// A table row.
    Row,
    /// A table cell.
    Cell,
    /// A tab list.
    TabList,
    /// A tab.
    Tab,
    /// A tab panel.
    TabPanel,
    /// A slider.
    Slider,
    /// A progress indicator.
    ProgressIndicator,
    /// A scroll bar.
    ScrollBar,
    /// A scrollable viewport.
    ScrollView,
    /// A splitter handle.
    Splitter,
    /// A heading.
    Heading,
    /// A link.
    Link,
    /// A tooltip.
    Tooltip,
    /// A document or editor surface.
    Document,
}

impl AccessibilityRole {
    const fn accesskit(self) -> accesskit::Role {
        match self {
            Self::GenericContainer => accesskit::Role::GenericContainer,
            Self::Application => accesskit::Role::Application,
            Self::Window => accesskit::Role::Window,
            Self::Dialog => accesskit::Role::Dialog,
            Self::Pane => accesskit::Role::Pane,
            Self::Group => accesskit::Role::Group,
            Self::Toolbar => accesskit::Role::Toolbar,
            Self::MenuBar => accesskit::Role::MenuBar,
            Self::Menu => accesskit::Role::Menu,
            Self::MenuItem => accesskit::Role::MenuItem,
            Self::Button => accesskit::Role::Button,
            Self::CheckBox => accesskit::Role::CheckBox,
            Self::RadioButton => accesskit::Role::RadioButton,
            Self::Switch => accesskit::Role::Switch,
            Self::TextField => accesskit::Role::TextInput,
            Self::MultilineTextField => accesskit::Role::MultilineTextInput,
            Self::Label => accesskit::Role::Label,
            Self::Image => accesskit::Role::Image,
            Self::List => accesskit::Role::List,
            Self::ListBox => accesskit::Role::ListBox,
            Self::ComboBox => accesskit::Role::ComboBox,
            Self::ListItem => accesskit::Role::ListItem,
            Self::Table => accesskit::Role::Table,
            Self::Row => accesskit::Role::Row,
            Self::Cell => accesskit::Role::Cell,
            Self::TabList => accesskit::Role::TabList,
            Self::Tab => accesskit::Role::Tab,
            Self::TabPanel => accesskit::Role::TabPanel,
            Self::Slider => accesskit::Role::Slider,
            Self::ProgressIndicator => accesskit::Role::ProgressIndicator,
            Self::ScrollBar => accesskit::Role::ScrollBar,
            Self::ScrollView => accesskit::Role::ScrollView,
            Self::Splitter => accesskit::Role::Splitter,
            Self::Heading => accesskit::Role::Heading,
            Self::Link => accesskit::Role::Link,
            Self::Tooltip => accesskit::Role::Tooltip,
            Self::Document => accesskit::Role::Document,
        }
    }
}

/// A compact set of semantic states for a node.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AccessibilityState(u32);

impl AccessibilityState {
    /// The node currently owns keyboard focus.
    pub const FOCUSED: Self = Self(1 << 0);
    /// The node can receive keyboard focus.
    pub const FOCUSABLE: Self = Self(1 << 1);
    /// The node is disabled and does not accept input.
    pub const DISABLED: Self = Self(1 << 2);
    /// The node is selected in a list, tab set, or table.
    pub const SELECTED: Self = Self(1 << 3);
    /// A checkbox, switch, or toggle button is checked.
    pub const CHECKED: Self = Self(1 << 4);
    /// A checkbox is in the mixed state.
    pub const MIXED: Self = Self(1 << 5);
    /// An expandable node is expanded.
    pub const EXPANDED: Self = Self(1 << 6);
    /// A text control is read-only.
    pub const READ_ONLY: Self = Self(1 << 7);
    /// A form control is required.
    pub const REQUIRED: Self = Self(1 << 8);
    /// The current value fails validation.
    pub const INVALID: Self = Self(1 << 9);
    /// The node is busy updating.
    pub const BUSY: Self = Self(1 << 10);
    /// A text control accepts multiple lines.
    pub const MULTILINE: Self = Self(1 << 11);
    /// A text control masks its value.
    pub const PASSWORD: Self = Self(1 << 12);
    /// A dialog is modal.
    pub const MODAL: Self = Self(1 << 13);
    /// A list supports more than one selected item.
    pub const MULTISELECTABLE: Self = Self(1 << 14);
    /// A control accepts touch exploration without interception.
    pub const TOUCH_TRANSPARENT: Self = Self(1 << 15);
    /// A control stays visible but is not interactive.
    pub const HIDDEN: Self = Self(1 << 16);

    /// Returns an empty state set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns whether every state in `other` is present.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Returns the union of two states.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns whether no state is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for AccessibilityState {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for AccessibilityState {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// Actions an assistive technology may invoke on a node.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AccessibilityActions(u32);

impl AccessibilityActions {
    /// Move keyboard focus to the node.
    pub const FOCUS: Self = Self(1 << 0);
    /// Activate the node as if it were clicked.
    pub const CLICK: Self = Self(1 << 1);
    /// Toggle a checkbox, switch, or toggle button.
    pub const TOGGLE: Self = Self(1 << 2);
    /// Replace a text or numeric value.
    pub const SET_VALUE: Self = Self(1 << 3);
    /// Increment a numeric value.
    pub const INCREMENT: Self = Self(1 << 4);
    /// Decrement a numeric value.
    pub const DECREMENT: Self = Self(1 << 5);
    /// Expand an expandable node.
    pub const EXPAND: Self = Self(1 << 6);
    /// Collapse an expandable node.
    pub const COLLAPSE: Self = Self(1 << 7);
    /// Scroll the node's content.
    pub const SCROLL: Self = Self(1 << 8);
    /// Select the node's content.
    pub const SELECT: Self = Self(1 << 9);

    /// Returns an empty action set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns whether every action in `other` is supported.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Returns the union of two action sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for AccessibilityActions {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for AccessibilityActions {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// A semantic action requested by an assistive technology.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AccessibilityAction {
    /// Move keyboard focus to the node.
    Focus,
    /// Activate the node.
    Click,
    /// Toggle a checkable control.
    Toggle,
    /// Replace the text value and reset the selection.
    SetValue(String),
    /// Increment a numeric value by one step.
    Increment,
    /// Decrement a numeric value by one step.
    Decrement,
    /// Expand a disclosure or tree item.
    Expand,
    /// Collapse a disclosure or tree item.
    Collapse,
    /// Scroll by a logical-pixel delta.
    ScrollBy(drawen_core::Point),
    /// Select the node or its text content.
    Select,
}

/// A text selection expressed as byte offsets into the accessible value.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AccessibilityTextSelection {
    /// The fixed end of the selection.
    pub anchor: usize,
    /// The active end of the selection.
    pub focus: usize,
}

/// Semantic description supplied by a widget.
#[derive(Clone, Debug, PartialEq)]
pub struct Accessibility {
    /// The node's role.
    pub role: AccessibilityRole,
    /// Accessible name. When omitted, the runtime derives one from child text.
    pub label: Option<String>,
    /// Optional longer description or help text.
    pub description: Option<String>,
    /// A textual value for text fields, combo boxes, and similar controls.
    pub value: Option<String>,
    /// A numeric value for sliders and progress indicators.
    pub numeric_value: Option<f64>,
    /// Lower bound for [`Self::numeric_value`].
    pub min_numeric_value: Option<f64>,
    /// Upper bound for [`Self::numeric_value`].
    pub max_numeric_value: Option<f64>,
    /// Semantic states.
    pub states: AccessibilityState,
    /// Supported semantic actions.
    pub actions: AccessibilityActions,
    /// Optional text selection in the value.
    pub text_selection: Option<AccessibilityTextSelection>,
    /// The node's position in the sequential tab order.
    ///
    /// Positive values sort before zero/unspecified values. Negative values
    /// remove the node from sequential traversal while still allowing it to be
    /// focused programmatically.
    pub tab_index: Option<i32>,
    /// Whether the node should be exposed in the semantic tree.
    pub hidden: bool,
}

impl Accessibility {
    /// Creates a semantic description with `role` and no name or value.
    #[must_use]
    pub const fn new(role: AccessibilityRole) -> Self {
        Self {
            role,
            label: None,
            description: None,
            value: None,
            numeric_value: None,
            min_numeric_value: None,
            max_numeric_value: None,
            states: AccessibilityState::empty(),
            actions: AccessibilityActions::empty(),
            text_selection: None,
            tab_index: None,
            hidden: false,
        }
    }

    /// Returns this description with an accessible name.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Returns this description with a description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Returns this description with a text value.
    #[must_use]
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Returns this description with a numeric value and bounds.
    #[must_use]
    pub fn with_numeric_value(mut self, value: f64, minimum: f64, maximum: f64) -> Self {
        self.numeric_value = Some(value);
        self.min_numeric_value = Some(minimum);
        self.max_numeric_value = Some(maximum);
        self
    }

    /// Returns this description with additional states.
    #[must_use]
    pub fn with_states(mut self, states: AccessibilityState) -> Self {
        self.states |= states;
        self
    }

    /// Returns this description with additional actions.
    #[must_use]
    pub fn with_actions(mut self, actions: AccessibilityActions) -> Self {
        self.actions |= actions;
        self
    }

    /// Returns this description with a text selection.
    #[must_use]
    pub const fn with_text_selection(mut self, anchor: usize, focus: usize) -> Self {
        self.text_selection = Some(AccessibilityTextSelection { anchor, focus });
        self
    }

    /// Returns this description with an explicit tab index.
    #[must_use]
    pub const fn with_tab_index(mut self, tab_index: i32) -> Self {
        self.tab_index = Some(tab_index);
        self
    }
}

/// One node in a semantic snapshot of the retained widget tree.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    id: WidgetId,
    parent: Option<WidgetId>,
    children: Vec<WidgetId>,
    role: AccessibilityRole,
    label: Option<String>,
    description: Option<String>,
    value: Option<String>,
    numeric_value: Option<f64>,
    min_numeric_value: Option<f64>,
    max_numeric_value: Option<f64>,
    states: AccessibilityState,
    actions: AccessibilityActions,
    text_selection: Option<AccessibilityTextSelection>,
    tab_index: Option<i32>,
    bounds: Rect,
}

impl SemanticNode {
    /// Returns the stable widget handle.
    #[must_use]
    pub const fn id(&self) -> WidgetId {
        self.id
    }

    /// Returns the semantic parent, if any.
    #[must_use]
    pub const fn parent(&self) -> Option<WidgetId> {
        self.parent
    }

    /// Returns child handles in tree order.
    #[must_use]
    pub fn children(&self) -> &[WidgetId] {
        &self.children
    }

    /// Returns the node's role.
    #[must_use]
    pub const fn role(&self) -> AccessibilityRole {
        self.role
    }

    /// Returns the accessible name.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the accessible description.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the textual value.
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// Returns the numeric value.
    #[must_use]
    pub const fn numeric_value(&self) -> Option<f64> {
        self.numeric_value
    }

    /// Returns the lower numeric bound.
    #[must_use]
    pub const fn min_numeric_value(&self) -> Option<f64> {
        self.min_numeric_value
    }

    /// Returns the upper numeric bound.
    #[must_use]
    pub const fn max_numeric_value(&self) -> Option<f64> {
        self.max_numeric_value
    }

    /// Returns semantic states.
    #[must_use]
    pub const fn states(&self) -> AccessibilityState {
        self.states
    }

    /// Returns supported semantic actions.
    #[must_use]
    pub const fn actions(&self) -> AccessibilityActions {
        self.actions
    }

    /// Returns the text selection, if any.
    #[must_use]
    pub const fn text_selection(&self) -> Option<AccessibilityTextSelection> {
        self.text_selection
    }

    /// Returns the explicit tab index.
    #[must_use]
    pub const fn tab_index(&self) -> Option<i32> {
        self.tab_index
    }

    /// Returns absolute bounds in logical pixels.
    #[must_use]
    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    fn accesskit_id(&self) -> accesskit::NodeId {
        accesskit_node_id(self.id)
    }

    /// Returns the AccessKit node identifier that represents this widget.
    #[must_use]
    pub fn accesskit_node_id(&self) -> accesskit::NodeId {
        self.accesskit_id()
    }
}

/// An immutable semantic snapshot of a [`crate::UiRuntime`] tree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticTree {
    nodes: Vec<SemanticNode>,
    indices: HashMap<WidgetId, usize>,
    root: Option<WidgetId>,
    focused: Option<WidgetId>,
}

impl SemanticTree {
    /// Returns the semantic root.
    #[must_use]
    pub const fn root(&self) -> Option<WidgetId> {
        self.root
    }

    /// Returns the widget that owns keyboard focus.
    #[must_use]
    pub const fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    /// Returns every semantic node in depth-first tree order.
    #[must_use]
    pub fn nodes(&self) -> &[SemanticNode] {
        &self.nodes
    }

    /// Looks up a semantic node by widget handle.
    #[must_use]
    pub fn node(&self, id: WidgetId) -> Option<&SemanticNode> {
        self.indices.get(&id).map(|index| &self.nodes[*index])
    }

    /// Returns the node that should receive focus for sequential traversal.
    #[must_use]
    pub fn next_focusable(&self, from: Option<WidgetId>, backwards: bool) -> Option<WidgetId> {
        let mut order: Vec<(WidgetId, i32, usize)> = self
            .nodes
            .iter()
            .filter(|node| {
                node.states.contains(AccessibilityState::FOCUSABLE)
                    && node.tab_index.is_none_or(|index| index >= 0)
            })
            .enumerate()
            .map(|(index, node)| (node.id, node.tab_index.unwrap_or(0), index))
            .collect();
        order.sort_by_key(|(_, tab_index, index)| (i32::from(*tab_index <= 0), *tab_index, *index));
        if order.is_empty() {
            return None;
        }
        let current = from.and_then(|from| order.iter().position(|(id, _, _)| *id == from));
        let index = match (current, backwards) {
            (Some(index), false) => (index + 1) % order.len(),
            (Some(0) | None, true) => order.len() - 1,
            (Some(index), true) => index - 1,
            (None, false) => 0,
        };
        Some(order[index].0)
    }

    /// Translates this snapshot into an AccessKit tree update.
    ///
    /// AccessKit owns the native bridges: UI Automation on Windows, AT-SPI on
    /// Linux, and the macOS Accessibility API. The adapter only has to publish
    /// the returned update.
    #[must_use]
    pub fn to_accesskit(&self) -> accesskit::TreeUpdate {
        let nodes = self
            .nodes
            .iter()
            .map(|node| (node.accesskit_id(), accesskit_node(node)))
            .collect();
        let root = self.root.map(accesskit_node_id);
        let focused = self
            .focused
            .map(accesskit_node_id)
            .or(root)
            .unwrap_or(accesskit::NodeId(0));
        let tree = root.map(|root| {
            let mut tree = accesskit::Tree::new(root);
            tree.toolkit_name = Some("Drawen".to_owned());
            tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_owned());
            tree
        });
        accesskit::TreeUpdate {
            nodes,
            tree,
            tree_id: accesskit::TreeId::ROOT,
            focus: focused,
        }
    }
}

fn accesskit_node_id(id: WidgetId) -> accesskit::NodeId {
    // Keep index and generation in separate halves so stale handles cannot
    // alias live ones, and reserve zero for AccessKit's root sentinel.
    let index = u64::from(id.index());
    let generation = u64::from(id.generation());
    accesskit::NodeId(((generation + 1) << 32) | (index + 1))
}

/// Recovers a Drawen widget handle from an AccessKit node identifier.
///
/// Returns `None` when `node_id` was not produced by [`SemanticTree`] — for
/// example, the root sentinel or an identifier from a different toolkit.
#[must_use]
pub fn widget_id_from_accesskit(node_id: accesskit::NodeId) -> Option<WidgetId> {
    let raw = node_id.0;
    let index = u32::try_from(raw & 0xffff_ffff).ok()?.checked_sub(1)?;
    let generation = u32::try_from((raw >> 32) & 0xffff_ffff)
        .ok()?
        .checked_sub(1)?;
    Some(WidgetId::new(index, generation))
}

/// Converts an AccessKit action request into a widget handle and Drawen action.
///
/// Platform adapters use this to route native accessibility requests to
/// [`crate::UiRuntime::perform_accessibility_action`]. Returns `None` when the
/// request targets an unknown node or uses an action Drawen does not model.
#[must_use]
pub fn accessibility_action_from_accesskit(
    request: &accesskit::ActionRequest,
) -> Option<(WidgetId, AccessibilityAction)> {
    let target = widget_id_from_accesskit(request.target_node)?;
    let action = match request.action {
        accesskit::Action::Focus => AccessibilityAction::Focus,
        accesskit::Action::Click => AccessibilityAction::Click,
        accesskit::Action::Expand => AccessibilityAction::Expand,
        accesskit::Action::Collapse => AccessibilityAction::Collapse,
        accesskit::Action::Increment => AccessibilityAction::Increment,
        accesskit::Action::Decrement => AccessibilityAction::Decrement,
        accesskit::Action::SetValue | accesskit::Action::ReplaceSelectedText => {
            let Some(accesskit::ActionData::Value(value)) = request.data.as_ref() else {
                return None;
            };
            AccessibilityAction::SetValue(value.to_string())
        }
        accesskit::Action::ScrollDown => AccessibilityAction::ScrollBy(Point::new(0.0, 40.0)),
        accesskit::Action::ScrollUp => AccessibilityAction::ScrollBy(Point::new(0.0, -40.0)),
        accesskit::Action::ScrollLeft => AccessibilityAction::ScrollBy(Point::new(-40.0, 0.0)),
        accesskit::Action::ScrollRight => AccessibilityAction::ScrollBy(Point::new(40.0, 0.0)),
        _ => return None,
    };
    Some((target, action))
}

#[allow(clippy::too_many_lines)]
fn accesskit_node(node: &SemanticNode) -> accesskit::Node {
    let mut result = accesskit::Node::new(node.role.accesskit());
    if let Some(label) = &node.label {
        result.set_label(label.clone());
    }
    if let Some(description) = &node.description {
        result.set_description(description.clone());
    }
    if let Some(value) = &node.value {
        result.set_value(value.clone());
    }
    if let Some(value) = node.numeric_value {
        result.set_numeric_value(value);
    }
    if let Some(value) = node.min_numeric_value {
        result.set_min_numeric_value(value);
    }
    if let Some(value) = node.max_numeric_value {
        result.set_max_numeric_value(value);
    }
    if !node.children.is_empty() {
        result.set_children(
            node.children
                .iter()
                .map(|child| accesskit_node_id(*child))
                .collect::<Vec<_>>(),
        );
    }
    result.set_bounds(accesskit::Rect {
        x0: f64::from(node.bounds.origin.x),
        y0: f64::from(node.bounds.origin.y),
        x1: f64::from(node.bounds.origin.x + node.bounds.size.width()),
        y1: f64::from(node.bounds.origin.y + node.bounds.size.height()),
    });
    if node.states.contains(AccessibilityState::HIDDEN) {
        result.set_hidden();
    }
    if node.states.contains(AccessibilityState::DISABLED) {
        result.set_disabled();
    }
    if node.states.contains(AccessibilityState::FOCUSABLE) {
        result.add_action(accesskit::Action::Focus);
    }
    if node.states.contains(AccessibilityState::READ_ONLY) {
        result.set_read_only();
    }
    if node.states.contains(AccessibilityState::REQUIRED) {
        result.set_required();
    }
    if node.states.contains(AccessibilityState::INVALID) {
        result.set_invalid(accesskit::Invalid::True);
    }
    if node.states.contains(AccessibilityState::BUSY) {
        result.set_busy();
    }
    if node.states.contains(AccessibilityState::MODAL) {
        result.set_modal();
    }
    if node.states.contains(AccessibilityState::MULTISELECTABLE) {
        result.set_multiselectable();
    }
    if node.states.contains(AccessibilityState::TOUCH_TRANSPARENT) {
        result.set_touch_transparent();
    }
    if node.states.contains(AccessibilityState::EXPANDED) {
        result.set_expanded(true);
    }
    if node.states.contains(AccessibilityState::SELECTED) {
        result.set_selected(true);
    }
    if node.states.contains(AccessibilityState::CHECKED) {
        result.set_toggled(accesskit::Toggled::True);
    }
    if node.states.contains(AccessibilityState::MIXED) {
        result.set_toggled(accesskit::Toggled::Mixed);
    }
    if node.actions.contains(AccessibilityActions::CLICK) {
        result.add_action(accesskit::Action::Click);
    }
    if node.actions.contains(AccessibilityActions::TOGGLE) {
        result.add_action(accesskit::Action::Click);
    }
    if node.actions.contains(AccessibilityActions::SET_VALUE) {
        result.add_action(accesskit::Action::SetValue);
        result.add_action(accesskit::Action::ReplaceSelectedText);
    }
    if node.actions.contains(AccessibilityActions::INCREMENT) {
        result.add_action(accesskit::Action::Increment);
    }
    if node.actions.contains(AccessibilityActions::DECREMENT) {
        result.add_action(accesskit::Action::Decrement);
    }
    if node.actions.contains(AccessibilityActions::EXPAND) {
        result.add_action(accesskit::Action::Expand);
    }
    if node.actions.contains(AccessibilityActions::COLLAPSE) {
        result.add_action(accesskit::Action::Collapse);
    }
    if node.actions.contains(AccessibilityActions::SCROLL) {
        result.add_action(accesskit::Action::ScrollDown);
        result.add_action(accesskit::Action::ScrollUp);
        result.add_action(accesskit::Action::ScrollLeft);
        result.add_action(accesskit::Action::ScrollRight);
    }
    if let Some(selection) = node.text_selection {
        result.set_text_selection(accesskit::TextSelection {
            anchor: accesskit::TextPosition {
                node: node.accesskit_id(),
                character_index: selection.anchor,
            },
            focus: accesskit::TextPosition {
                node: node.accesskit_id(),
                character_index: selection.focus,
            },
        });
    }
    result
}

pub(crate) struct SemanticsBuilder {
    nodes: Vec<SemanticNode>,
    indices: HashMap<WidgetId, usize>,
    focused: Option<WidgetId>,
}

impl SemanticsBuilder {
    pub(crate) fn new(focused: Option<WidgetId>) -> Self {
        Self {
            nodes: Vec::new(),
            indices: HashMap::new(),
            focused,
        }
    }

    pub(crate) fn push(
        &mut self,
        id: WidgetId,
        parent: Option<WidgetId>,
        bounds: Rect,
        mut accessibility: Accessibility,
    ) {
        if self.focused == Some(id) {
            accessibility.states |= AccessibilityState::FOCUSED;
            accessibility.states |= AccessibilityState::FOCUSABLE;
        }
        if accessibility.hidden {
            accessibility.states |= AccessibilityState::HIDDEN;
        }
        let index = self.nodes.len();
        self.indices.insert(id, index);
        self.nodes.push(SemanticNode {
            id,
            parent,
            children: Vec::new(),
            role: accessibility.role,
            label: accessibility.label,
            description: accessibility.description,
            value: accessibility.value,
            numeric_value: accessibility.numeric_value,
            min_numeric_value: accessibility.min_numeric_value,
            max_numeric_value: accessibility.max_numeric_value,
            states: accessibility.states,
            actions: accessibility.actions,
            text_selection: accessibility.text_selection,
            tab_index: accessibility.tab_index,
            bounds,
        });
    }

    pub(crate) fn finish(mut self, root: Option<WidgetId>) -> SemanticTree {
        // Children are recorded after their parent. Link them in the same pass
        // once all identities are known.
        for index in 0..self.nodes.len() {
            if let Some(parent) = self.nodes[index].parent
                && let Some(parent_index) = self.indices.get(&parent).copied()
            {
                let child = self.nodes[index].id;
                self.nodes[parent_index].children.push(child);
            }
        }
        // Derive names for controls whose accessible label was not explicit.
        // This mirrors the HTML "name from contents" rule for simple controls.
        for index in 0..self.nodes.len() {
            if self.nodes[index].label.is_some() {
                continue;
            }
            let role = self.nodes[index].role;
            if !matches!(
                role,
                AccessibilityRole::Button
                    | AccessibilityRole::CheckBox
                    | AccessibilityRole::RadioButton
                    | AccessibilityRole::Switch
                    | AccessibilityRole::Tab
                    | AccessibilityRole::MenuItem
                    | AccessibilityRole::ListItem
            ) {
                continue;
            }
            let label = content_label(&self.nodes, &self.indices, self.nodes[index].id);
            self.nodes[index].label = label;
        }
        SemanticTree {
            nodes: self.nodes,
            indices: self.indices,
            root,
            focused: self.focused,
        }
    }
}

fn content_label(
    nodes: &[SemanticNode],
    indices: &HashMap<WidgetId, usize>,
    id: WidgetId,
) -> Option<String> {
    let mut parts = Vec::new();
    collect_labels(nodes, indices, id, &mut parts, 0);
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn collect_labels(
    nodes: &[SemanticNode],
    indices: &HashMap<WidgetId, usize>,
    id: WidgetId,
    parts: &mut Vec<String>,
    depth: usize,
) {
    if depth > 4 {
        return;
    }
    let Some(node) = indices.get(&id).map(|index| &nodes[*index]) else {
        return;
    };
    if node.role == AccessibilityRole::Label {
        if let Some(value) = node.value.as_ref().or(node.label.as_ref()) {
            parts.push(value.clone());
        }
        return;
    }
    for child in &node.children {
        collect_labels(nodes, indices, *child, parts, depth + 1);
    }
}
