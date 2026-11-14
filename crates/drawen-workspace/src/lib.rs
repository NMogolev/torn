//! Persistent, widget-independent docking and document workspace layouts.
//!
//! Applications own their panel factories and configuration files. This crate
//! owns only serializable placement data and validates every mutation before it
//! replaces the current layout.

use std::{collections::HashSet, fmt};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The first supported on-disk workspace-layout format.
pub const WORKSPACE_LAYOUT_VERSION: u32 = 1;

const MIN_SPLIT_RATIO: f32 = 0.1;
const MAX_SPLIT_RATIO: f32 = 0.9;

/// Default logical-pixel extent of a floating window kept inside a work area.
///
/// This is the minimum part of a floating panel or free-form document that
/// [`WorkspaceLayout::clamp_floating_bounds`] keeps visible in each axis.
pub const MIN_VISIBLE_FLOATING_EXTENT: f32 = 32.0;

/// Stable application-defined identifier of a tool panel.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PanelId(String);

impl PanelId {
    /// Creates a panel identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the application-defined identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for PanelId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for PanelId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for PanelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Stable application-defined identifier of a document.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DocumentId(String);

impl DocumentId {
    /// Creates a document identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the application-defined identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DocumentId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DocumentId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Orientation of a dock split.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockAxis {
    /// The first and second branches are laid out from left to right.
    Horizontal,
    /// The first and second branches are laid out from top to bottom.
    Vertical,
}

/// Which branch receives a panel when creating a split.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitPlacement {
    /// Put the moved panel before the target branch.
    First,
    /// Put the moved panel after the target branch.
    Second,
}

/// Presentation mode for the central document area.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DocumentPresentation {
    /// Documents are displayed as tabs in the central area.
    #[default]
    Tabbed,
    /// Documents can be displayed as bounded child windows.
    Freeform,
}

/// A logical-pixel rectangle used by a floating workspace item.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkspaceBounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl WorkspaceBounds {
    /// Creates finite bounds with non-negative dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::InvalidBounds`] if a coordinate is non-finite
    /// or a dimension is negative or non-finite.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Result<Self, WorkspaceError> {
        let bounds = Self {
            x,
            y,
            width,
            height,
        };
        bounds.ensure_valid()?;
        Ok(bounds)
    }

    /// Returns the logical x coordinate.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the logical y coordinate.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    /// Returns the logical width.
    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    /// Returns the logical height.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }

    fn ensure_valid(self) -> Result<(), WorkspaceError> {
        (self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0)
            .then_some(())
            .ok_or(WorkspaceError::InvalidBounds)
    }
}

/// A floating panel that is not represented in the docked tree.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct FloatingPanel {
    /// Stable panel identifier.
    pub id: PanelId,
    /// Bounds in logical pixels.
    pub bounds: WorkspaceBounds,
    /// Whether the floating window is currently visible.
    pub visible: bool,
    /// Relative stacking order among floating panels.
    pub z_order: u32,
}

/// A free-form document child window outside the docked tree.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct FloatingDocument {
    /// Stable document identifier.
    pub id: DocumentId,
    /// Bounds in logical pixels.
    pub bounds: WorkspaceBounds,
    /// Whether the document child window is currently visible.
    pub visible: bool,
    /// Relative stacking order among free-form documents.
    pub z_order: u32,
}

/// A node in the docked workspace tree.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Debug, PartialEq)]
pub enum LayoutNode {
    /// Two docked branches split along an axis.
    Split {
        /// Split orientation.
        axis: DockAxis,
        /// Normalized first-branch proportion, in the inclusive `0.1..=0.9` range.
        ratio: f32,
        /// First branch.
        first: Box<Self>,
        /// Second branch.
        second: Box<Self>,
    },
    /// An ordered group of dock tabs.
    Tabs {
        /// Ordered panel IDs.
        items: Vec<PanelId>,
        /// Selected panel ID, if one exists.
        active: Option<PanelId>,
    },
    /// One docked tool panel.
    Panel {
        /// Stable panel ID.
        id: PanelId,
    },
    /// The central document region.
    Documents {
        /// How documents are currently presented.
        presentation: DocumentPresentation,
        /// Ordered docked document IDs.
        items: Vec<DocumentId>,
        /// Selected document ID, if one exists.
        active: Option<DocumentId>,
    },
    /// An unoccupied location in the dock tree.
    ///
    /// Empty split branches and empty tab groups collapse during
    /// normalization, so this is mainly useful as an explicit empty root.
    Empty,
}

impl LayoutNode {
    /// Creates a dock split with a normalized ratio.
    #[must_use]
    pub fn split(axis: DockAxis, ratio: f32, first: Self, second: Self) -> Self {
        Self::Split {
            axis,
            ratio: normalize_ratio(ratio),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Creates a tab group and selects its first item, when present.
    #[must_use]
    pub fn tabs(items: Vec<PanelId>) -> Self {
        let active = items.first().cloned();
        Self::Tabs { items, active }
    }

    /// Creates the central tabbed document region.
    #[must_use]
    pub fn documents(items: Vec<DocumentId>) -> Self {
        let active = items.first().cloned();
        Self::Documents {
            presentation: DocumentPresentation::Tabbed,
            items,
            active,
        }
    }
}

/// A persisted desktop workspace, independent from live widgets and callbacks.
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceLayout {
    version: u32,
    root: LayoutNode,
    floating: Vec<FloatingPanel>,
    hidden: Vec<PanelId>,
    freeform_documents: Vec<FloatingDocument>,
}

impl WorkspaceLayout {
    /// Creates and validates the initial workspace layout.
    ///
    /// Split ratios are normalized and invalid selected tab IDs are repaired
    /// before validation.
    ///
    /// # Errors
    ///
    /// Returns an error when panel or document IDs occur more than once, a
    /// bounds value is invalid, or the tree is structurally inconsistent.
    pub fn new(root: LayoutNode) -> Result<Self, WorkspaceError> {
        let mut layout = Self {
            version: WORKSPACE_LAYOUT_VERSION,
            root,
            floating: Vec::new(),
            hidden: Vec::new(),
            freeform_documents: Vec::new(),
        };
        layout.normalize();
        layout.validate()?;
        Ok(layout)
    }

    /// Returns the persisted format version.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Returns the root of the docked-content tree.
    #[must_use]
    pub const fn root(&self) -> &LayoutNode {
        &self.root
    }

    /// Returns all floating panels.
    #[must_use]
    pub fn floating(&self) -> &[FloatingPanel] {
        &self.floating
    }

    /// Returns panel IDs hidden from the docked and floating areas.
    #[must_use]
    pub fn hidden(&self) -> &[PanelId] {
        &self.hidden
    }

    /// Returns all free-form document child windows.
    #[must_use]
    pub fn freeform_documents(&self) -> &[FloatingDocument] {
        &self.freeform_documents
    }

    /// Replaces the layout only when `candidate` is valid and supported.
    ///
    /// This is useful for applications that load data from their own storage.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported format version or an invalid layout.
    pub fn restore(&mut self, candidate: Self) -> Result<(), WorkspaceError> {
        *self = migrate_layout(candidate)?;
        Ok(())
    }

    /// Splits `target` and puts `panel` into the requested branch.
    ///
    /// A previously docked, hidden, or floating `panel` is first removed from
    /// its old location. An unplaced ID is also accepted, allowing an
    /// application registry to add a newly available panel.
    ///
    /// # Errors
    ///
    /// Returns an error if the source equals the target or the target is absent.
    pub fn split_panel(
        &mut self,
        target: &PanelId,
        panel: PanelId,
        axis: DockAxis,
        ratio: f32,
        placement: SplitPlacement,
    ) -> Result<(), WorkspaceError> {
        if target == &panel {
            return Err(WorkspaceError::SameSourceAndTarget);
        }
        self.apply_transaction(|candidate| {
            candidate.detach_panel(&panel);
            split_at_panel(&mut candidate.root, target, panel, axis, ratio, placement)
                .then_some(())
                .ok_or_else(|| WorkspaceError::PanelNotFound(target.clone()))
        })
    }

    /// Moves `panel` into a tab group with `target` and activates it.
    ///
    /// An unplaced `panel` ID is accepted so a registered panel can be added to
    /// a workspace for the first time.
    ///
    /// # Errors
    ///
    /// Returns an error if the source equals the target or the target is absent.
    pub fn move_to_tabs(&mut self, target: &PanelId, panel: PanelId) -> Result<(), WorkspaceError> {
        if target == &panel {
            return Err(WorkspaceError::SameSourceAndTarget);
        }
        self.apply_transaction(|candidate| {
            candidate.detach_panel(&panel);
            tab_with_panel(&mut candidate.root, target, panel)
                .then_some(())
                .ok_or_else(|| WorkspaceError::PanelNotFound(target.clone()))
        })
    }

    /// Docks `panel` as a tab beside `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if the source equals the target or the target is absent.
    pub fn dock_panel(&mut self, target: &PanelId, panel: PanelId) -> Result<(), WorkspaceError> {
        self.move_to_tabs(target, panel)
    }

    /// Moves a docked panel into a visible floating window.
    ///
    /// # Errors
    ///
    /// Returns an error if `panel` is not docked or its bounds are invalid.
    pub fn float_panel(
        &mut self,
        panel: &PanelId,
        bounds: WorkspaceBounds,
    ) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            if !candidate.detach_panel(panel) {
                return Err(WorkspaceError::PanelNotFound(panel.clone()));
            }
            candidate.floating.push(FloatingPanel {
                id: panel.clone(),
                bounds,
                visible: true,
                z_order: next_z_order(&candidate.floating),
            });
            Ok(())
        })
    }

    /// Moves a floating panel to a new logical position.
    ///
    /// The panel keeps its current size. Use
    /// [`Self::resize_floating_panel`] to change the extent.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::PanelNotFloating`] if `panel` is not floating,
    /// or [`WorkspaceError::InvalidBounds`] if a coordinate is not finite.
    pub fn move_floating_panel(
        &mut self,
        panel: &PanelId,
        x: f32,
        y: f32,
    ) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            let index = candidate
                .floating
                .iter()
                .position(|item| &item.id == panel)
                .ok_or_else(|| WorkspaceError::PanelNotFloating(panel.clone()))?;
            let current = candidate.floating[index].bounds;
            candidate.floating[index].bounds =
                WorkspaceBounds::new(x, y, current.width, current.height)?;
            Ok(())
        })
    }

    /// Resizes a floating panel while keeping its logical origin.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::PanelNotFloating`] if `panel` is not floating,
    /// or [`WorkspaceError::InvalidBounds`] if an extent is negative or
    /// non-finite.
    pub fn resize_floating_panel(
        &mut self,
        panel: &PanelId,
        width: f32,
        height: f32,
    ) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            let index = candidate
                .floating
                .iter()
                .position(|item| &item.id == panel)
                .ok_or_else(|| WorkspaceError::PanelNotFloating(panel.clone()))?;
            let current = candidate.floating[index].bounds;
            candidate.floating[index].bounds =
                WorkspaceBounds::new(current.x, current.y, width, height)?;
            Ok(())
        })
    }

    /// Raises a floating panel above every other floating panel.
    ///
    /// Z-orders are normalized after every transaction, so the counter stays
    /// dense and the stacking order is always unambiguous.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::PanelNotFloating`] if `panel` is not floating.
    pub fn bring_panel_to_front(&mut self, panel: &PanelId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            let index = candidate
                .floating
                .iter()
                .position(|item| &item.id == panel)
                .ok_or_else(|| WorkspaceError::PanelNotFloating(panel.clone()))?;
            bring_index_to_front(&mut candidate.floating, index);
            Ok(())
        })
    }

    /// Permanently removes a panel from every part of the layout.
    ///
    /// This is the panel lifecycle counterpart to [`Self::close_document`].
    /// The ID is freed from docked, floating, and hidden storage. If the panel
    /// was the active tab, the next tab (or the previous one at the end) becomes
    /// active.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::PanelNotFound`] if `panel` is not placed in
    /// the layout.
    pub fn remove_panel(&mut self, panel: &PanelId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            candidate
                .detach_panel(panel)
                .then_some(())
                .ok_or_else(|| WorkspaceError::PanelNotFound(panel.clone()))
        })
    }

    /// Hides a panel regardless of whether it is docked or floating.
    ///
    /// # Errors
    ///
    /// Returns an error if `panel` is neither docked nor floating.
    pub fn hide_panel(&mut self, panel: &PanelId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            if !candidate.detach_panel(panel) {
                return Err(WorkspaceError::PanelNotFound(panel.clone()));
            }
            candidate.hidden.push(panel.clone());
            Ok(())
        })
    }

    /// Shows a hidden panel as a tab beside `target`.
    ///
    /// # Errors
    ///
    /// Returns an error if `panel` is not hidden or `target` is absent.
    pub fn show_panel(&mut self, panel: &PanelId, target: &PanelId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            if !take_id(&mut candidate.hidden, panel) {
                return Err(WorkspaceError::PanelNotHidden(panel.clone()));
            }
            tab_with_panel(&mut candidate.root, target, panel.clone())
                .then_some(())
                .ok_or_else(|| WorkspaceError::PanelNotFound(target.clone()))
        })
    }

    /// Selects a panel in its tab group.
    ///
    /// # Errors
    ///
    /// Returns an error if `panel` is not docked.
    pub fn activate_panel(&mut self, panel: &PanelId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            activate_panel(&mut candidate.root, panel)
                .then_some(())
                .ok_or_else(|| WorkspaceError::PanelNotFound(panel.clone()))
        })
    }

    /// Resizes the innermost dock split that contains `panel`.
    ///
    /// This lets a projected workspace view resize a splitter without exposing
    /// mutable layout nodes. The ratio is normalized to the supported
    /// `0.1..=0.9` range.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::PanelNotFound`] when `panel` is not docked
    /// beneath a split.
    pub fn resize_split_for_panel(
        &mut self,
        panel: &PanelId,
        ratio: f32,
    ) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            resize_split_for_panel(&mut candidate.root, panel, ratio)
                .then_some(())
                .ok_or_else(|| WorkspaceError::PanelNotFound(panel.clone()))
        })
    }

    /// Moves a docked document into a visible free-form child window.
    ///
    /// # Errors
    ///
    /// Returns an error if `document` is not in the document area or bounds are invalid.
    pub fn float_document(
        &mut self,
        document: &DocumentId,
        bounds: WorkspaceBounds,
    ) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            if !detach_document(&mut candidate.root, document) {
                return Err(WorkspaceError::DocumentNotFound(document.clone()));
            }
            candidate.freeform_documents.push(FloatingDocument {
                id: document.clone(),
                bounds,
                visible: true,
                z_order: next_z_order(&candidate.freeform_documents),
            });
            Ok(())
        })
    }

    /// Returns a free-form document to the central document area and activates it.
    ///
    /// # Errors
    ///
    /// Returns an error if `document` is not free-form or there is no document area.
    pub fn dock_document(&mut self, document: &DocumentId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            if !take_document(&mut candidate.freeform_documents, document) {
                return Err(WorkspaceError::DocumentNotFound(document.clone()));
            }
            insert_document(&mut candidate.root, document.clone())
                .then_some(())
                .ok_or(WorkspaceError::DocumentsAreaNotFound)
        })
    }

    /// Moves a free-form document window to a new logical position.
    ///
    /// The document keeps its current size. Use
    /// [`Self::resize_floating_document`] to change the extent.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::DocumentNotFloating`] if `document` is not
    /// free-form, or [`WorkspaceError::InvalidBounds`] if a coordinate is not
    /// finite.
    pub fn move_floating_document(
        &mut self,
        document: &DocumentId,
        x: f32,
        y: f32,
    ) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            let index = candidate
                .freeform_documents
                .iter()
                .position(|item| &item.id == document)
                .ok_or_else(|| WorkspaceError::DocumentNotFloating(document.clone()))?;
            let current = candidate.freeform_documents[index].bounds;
            candidate.freeform_documents[index].bounds =
                WorkspaceBounds::new(x, y, current.width, current.height)?;
            Ok(())
        })
    }

    /// Resizes a free-form document window while keeping its logical origin.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::DocumentNotFloating`] if `document` is not
    /// free-form, or [`WorkspaceError::InvalidBounds`] if an extent is negative
    /// or non-finite.
    pub fn resize_floating_document(
        &mut self,
        document: &DocumentId,
        width: f32,
        height: f32,
    ) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            let index = candidate
                .freeform_documents
                .iter()
                .position(|item| &item.id == document)
                .ok_or_else(|| WorkspaceError::DocumentNotFloating(document.clone()))?;
            let current = candidate.freeform_documents[index].bounds;
            candidate.freeform_documents[index].bounds =
                WorkspaceBounds::new(current.x, current.y, width, height)?;
            Ok(())
        })
    }

    /// Raises a free-form document above every other free-form document.
    ///
    /// Z-orders are normalized after every transaction, so the counter stays
    /// dense and the stacking order is always unambiguous.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::DocumentNotFloating`] if `document` is not
    /// free-form.
    pub fn bring_document_to_front(&mut self, document: &DocumentId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            let index = candidate
                .freeform_documents
                .iter()
                .position(|item| &item.id == document)
                .ok_or_else(|| WorkspaceError::DocumentNotFloating(document.clone()))?;
            bring_index_to_front(&mut candidate.freeform_documents, index);
            Ok(())
        })
    }

    /// Permanently closes a document, whether docked or free-form.
    ///
    /// The ID is freed from both the central document area and free-form
    /// storage. Closing the active document selects the next tab, or the
    /// previous tab when the closed document was last.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::DocumentNotFound`] if `document` is not placed
    /// in the layout.
    pub fn close_document(&mut self, document: &DocumentId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            let docked = detach_document(&mut candidate.root, document);
            let freeform = take_document(&mut candidate.freeform_documents, document);
            (docked || freeform)
                .then_some(())
                .ok_or_else(|| WorkspaceError::DocumentNotFound(document.clone()))
        })
    }

    /// Selects a document in the central document area or raises a free-form one.
    ///
    /// # Errors
    ///
    /// Returns an error if `document` is neither docked nor free-form.
    pub fn activate_document(&mut self, document: &DocumentId) -> Result<(), WorkspaceError> {
        self.apply_transaction(|candidate| {
            if activate_document(&mut candidate.root, document) {
                return Ok(());
            }
            let z_order = next_z_order(&candidate.freeform_documents);
            if let Some(item) = candidate
                .freeform_documents
                .iter_mut()
                .find(|item| &item.id == document)
            {
                item.z_order = z_order;
                return Ok(());
            }
            Err(WorkspaceError::DocumentNotFound(document.clone()))
        })
    }

    /// Clamps all floating windows to `work_area` after a monitor change.
    ///
    /// The platform adapter owns monitor discovery, DPI conversion, and
    /// disconnected-monitor handling. It calls this method with the current
    /// monitor work area. Window sizes are preserved, but every window is moved
    /// until at least [`MIN_VISIBLE_FLOATING_EXTENT`] logical pixels of its
    /// width and height remain inside the work area. The guaranteed extent is
    /// reduced automatically when a window is smaller than the default.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::InvalidBounds`] when `work_area` is not a
    /// finite, non-negative rectangle.
    pub fn clamp_floating_bounds(
        &mut self,
        work_area: WorkspaceBounds,
    ) -> Result<(), WorkspaceError> {
        self.clamp_floating_bounds_with_min_visible(work_area, MIN_VISIBLE_FLOATING_EXTENT)
    }

    /// Clamps all floating windows using an explicit minimum visible extent.
    ///
    /// A `min_visible_extent` of zero permits windows to remain fully outside
    /// the work area; use [`Self::clamp_floating_bounds`] for the normal restore
    /// path.
    ///
    /// # Errors
    ///
    /// Returns [`WorkspaceError::InvalidBounds`] when `work_area` is not a
    /// finite, non-negative rectangle, or
    /// [`WorkspaceError::InvalidVisibleExtent`] when `min_visible_extent` is
    /// negative or non-finite.
    pub fn clamp_floating_bounds_with_min_visible(
        &mut self,
        work_area: WorkspaceBounds,
        min_visible_extent: f32,
    ) -> Result<(), WorkspaceError> {
        work_area.ensure_valid()?;
        if !min_visible_extent.is_finite() || min_visible_extent < 0.0 {
            return Err(WorkspaceError::InvalidVisibleExtent);
        }
        self.apply_transaction(|candidate| {
            for item in &mut candidate.floating {
                item.bounds = clamp_bounds(item.bounds, work_area, min_visible_extent);
            }
            for item in &mut candidate.freeform_documents {
                item.bounds = clamp_bounds(item.bounds, work_area, min_visible_extent);
            }
            Ok(())
        })
    }

    /// Validates uniqueness, tree structure, split ratios, and floating bounds.
    ///
    /// # Errors
    ///
    /// Returns an error when any workspace invariant is violated.
    pub fn validate(&self) -> Result<(), WorkspaceError> {
        let mut panels = HashSet::new();
        let mut documents = HashSet::new();
        let mut document_areas = 0;
        validate_node(&self.root, &mut panels, &mut documents, &mut document_areas)?;
        if document_areas > 1 {
            return Err(WorkspaceError::MultipleDocumentAreas);
        }

        for item in &self.floating {
            item.bounds.ensure_valid()?;
            insert_unique_panel(&mut panels, &item.id)?;
        }
        for id in &self.hidden {
            insert_unique_panel(&mut panels, id)?;
        }
        for item in &self.freeform_documents {
            item.bounds.ensure_valid()?;
            insert_unique_document(&mut documents, &item.id)?;
        }
        Ok(())
    }

    fn normalize(&mut self) {
        normalize_node(&mut self.root);
        normalize_z_order(&mut self.floating);
        normalize_z_order(&mut self.freeform_documents);
    }

    fn apply_transaction(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<(), WorkspaceError>,
    ) -> Result<(), WorkspaceError> {
        let mut candidate = self.clone();
        operation(&mut candidate)?;
        candidate.normalize();
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn detach_panel(&mut self, panel: &PanelId) -> bool {
        let from_root = detach_panel_from_node(&mut self.root, panel);
        let from_floating = take_panel(&mut self.floating, panel);
        let from_hidden = take_id(&mut self.hidden, panel);
        from_root || from_floating || from_hidden
    }

    /// Encodes this layout as readable, versioned JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization fails.
    #[cfg(feature = "serde")]
    pub fn to_json(&self) -> Result<String, WorkspaceSerializationError> {
        serde_json::to_string_pretty(self).map_err(WorkspaceSerializationError::Encode)
    }

    /// Decodes, normalizes, and validates readable, versioned JSON.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON, unsupported versions, or invalid layouts.
    #[cfg(feature = "serde")]
    pub fn from_json(json: &str) -> Result<Self, WorkspaceSerializationError> {
        let layout: Self =
            serde_json::from_str(json).map_err(WorkspaceSerializationError::Decode)?;
        migrate_layout(layout).map_err(WorkspaceSerializationError::Workspace)
    }
}

/// Why a workspace operation or validation failed.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceError {
    /// A panel ID occurs in more than one workspace location.
    DuplicatePanel(PanelId),
    /// A document ID occurs in more than one workspace location.
    DuplicateDocument(DocumentId),
    /// A dock operation referred to a panel that is not in the docked tree.
    PanelNotFound(PanelId),
    /// A floating operation referred to a panel that is not floating.
    PanelNotFloating(PanelId),
    /// A show operation referred to a panel that is not hidden.
    PanelNotHidden(PanelId),
    /// A document operation referred to an unknown document.
    DocumentNotFound(DocumentId),
    /// A floating operation referred to a document that is not free-form.
    DocumentNotFloating(DocumentId),
    /// The layout has no central document area.
    DocumentsAreaNotFound,
    /// The layout contains more than one central document area.
    MultipleDocumentAreas,
    /// A split ratio is not finite or is outside the normalized range.
    InvalidSplitRatio,
    /// A floating item's bounds are not finite or have a negative extent.
    InvalidBounds,
    /// The requested minimum visible extent is negative or non-finite.
    InvalidVisibleExtent,
    /// A move attempted to use the same panel as both source and target.
    SameSourceAndTarget,
    /// The input uses an on-disk format newer than this crate supports.
    UnsupportedVersion(u32),
    /// An older format has no migration registered by this crate version.
    MigrationRequired(u32),
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePanel(id) => write!(formatter, "panel `{id}` appears more than once"),
            Self::DuplicateDocument(id) => {
                write!(formatter, "document `{id}` appears more than once")
            }
            Self::PanelNotFound(id) => write!(formatter, "panel `{id}` was not found"),
            Self::PanelNotFloating(id) => write!(formatter, "panel `{id}` is not floating"),
            Self::PanelNotHidden(id) => write!(formatter, "panel `{id}` is not hidden"),
            Self::DocumentNotFound(id) => write!(formatter, "document `{id}` was not found"),
            Self::DocumentNotFloating(id) => {
                write!(formatter, "document `{id}` is not free-form")
            }
            Self::DocumentsAreaNotFound => formatter.write_str("layout has no document area"),
            Self::MultipleDocumentAreas => {
                formatter.write_str("layout has more than one document area")
            }
            Self::InvalidSplitRatio => {
                formatter.write_str("split ratio must be finite and normalized")
            }
            Self::InvalidBounds => {
                formatter.write_str("floating bounds must be finite and non-negative")
            }
            Self::InvalidVisibleExtent => {
                formatter.write_str("minimum visible extent must be finite and non-negative")
            }
            Self::SameSourceAndTarget => {
                formatter.write_str("source panel and target panel must differ")
            }
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "workspace format version {version} is newer than supported"
            ),
            Self::MigrationRequired(version) => write!(
                formatter,
                "workspace format version {version} requires a migration"
            ),
        }
    }
}

impl std::error::Error for WorkspaceError {}

/// Why JSON workspace serialization or restoration failed.
#[cfg(feature = "serde")]
#[derive(Debug)]
pub enum WorkspaceSerializationError {
    /// JSON encoding failed.
    Encode(serde_json::Error),
    /// JSON decoding failed.
    Decode(serde_json::Error),
    /// Decoded JSON failed workspace validation or version checks.
    Workspace(WorkspaceError),
}

#[cfg(feature = "serde")]
impl fmt::Display for WorkspaceSerializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => write!(formatter, "could not encode workspace JSON: {error}"),
            Self::Decode(error) => write!(formatter, "could not decode workspace JSON: {error}"),
            Self::Workspace(error) => error.fmt(formatter),
        }
    }
}

#[cfg(feature = "serde")]
impl std::error::Error for WorkspaceSerializationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encode(error) | Self::Decode(error) => Some(error),
            Self::Workspace(error) => Some(error),
        }
    }
}

fn migrate_layout(mut layout: WorkspaceLayout) -> Result<WorkspaceLayout, WorkspaceError> {
    if layout.version > WORKSPACE_LAYOUT_VERSION {
        return Err(WorkspaceError::UnsupportedVersion(layout.version));
    }
    if layout.version < WORKSPACE_LAYOUT_VERSION {
        // Version 1 is the oldest format. Future format versions add explicit
        // upgrade steps here before normalization and validation.
        return Err(WorkspaceError::MigrationRequired(layout.version));
    }
    layout.normalize();
    layout.validate()?;
    Ok(layout)
}

fn normalize_ratio(ratio: f32) -> f32 {
    if ratio.is_finite() {
        ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO)
    } else {
        0.5
    }
}

fn normalize_node(node: &mut LayoutNode) {
    let mut current = std::mem::replace(node, LayoutNode::Empty);
    match &mut current {
        LayoutNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            *ratio = normalize_ratio(*ratio);
            normalize_node(first);
            normalize_node(second);
            if matches!(first.as_ref(), LayoutNode::Empty) {
                *node = std::mem::replace(second.as_mut(), LayoutNode::Empty);
                return;
            }
            if matches!(second.as_ref(), LayoutNode::Empty) {
                *node = std::mem::replace(first.as_mut(), LayoutNode::Empty);
                return;
            }
        }
        LayoutNode::Tabs { items, active } => {
            match items.as_slice() {
                [] => {
                    *node = LayoutNode::Empty;
                    return;
                }
                [id] => {
                    *node = LayoutNode::Panel { id: id.clone() };
                    return;
                }
                _ => {}
            }
            if active.as_ref().is_none_or(|id| !items.contains(id)) {
                *active = items.first().cloned();
            }
        }
        LayoutNode::Documents { items, active, .. } => {
            if active.as_ref().is_none_or(|id| !items.contains(id)) {
                *active = items.first().cloned();
            }
        }
        LayoutNode::Panel { .. } | LayoutNode::Empty => {}
    }
    *node = current;
}

fn validate_node(
    node: &LayoutNode,
    panels: &mut HashSet<PanelId>,
    documents: &mut HashSet<DocumentId>,
    document_areas: &mut u32,
) -> Result<(), WorkspaceError> {
    match node {
        LayoutNode::Split {
            ratio,
            first,
            second,
            ..
        } => {
            if !ratio.is_finite() || !(MIN_SPLIT_RATIO..=MAX_SPLIT_RATIO).contains(ratio) {
                return Err(WorkspaceError::InvalidSplitRatio);
            }
            validate_node(first, panels, documents, document_areas)?;
            validate_node(second, panels, documents, document_areas)?;
        }
        LayoutNode::Tabs { items, active } => {
            if active.as_ref().is_some_and(|id| !items.contains(id)) {
                return Err(WorkspaceError::PanelNotFound(
                    active.clone().expect("active is known to be present"),
                ));
            }
            for id in items {
                insert_unique_panel(panels, id)?;
            }
        }
        LayoutNode::Panel { id } => insert_unique_panel(panels, id)?,
        LayoutNode::Documents { items, active, .. } => {
            *document_areas += 1;
            if active.as_ref().is_some_and(|id| !items.contains(id)) {
                return Err(WorkspaceError::DocumentNotFound(
                    active.clone().expect("active is known to be present"),
                ));
            }
            for id in items {
                insert_unique_document(documents, id)?;
            }
        }
        LayoutNode::Empty => {}
    }
    Ok(())
}

fn insert_unique_panel(ids: &mut HashSet<PanelId>, id: &PanelId) -> Result<(), WorkspaceError> {
    ids.insert(id.clone())
        .then_some(())
        .ok_or_else(|| WorkspaceError::DuplicatePanel(id.clone()))
}

fn insert_unique_document(
    ids: &mut HashSet<DocumentId>,
    id: &DocumentId,
) -> Result<(), WorkspaceError> {
    ids.insert(id.clone())
        .then_some(())
        .ok_or_else(|| WorkspaceError::DuplicateDocument(id.clone()))
}

fn split_at_panel(
    node: &mut LayoutNode,
    target: &PanelId,
    panel: PanelId,
    axis: DockAxis,
    ratio: f32,
    placement: SplitPlacement,
) -> bool {
    match node {
        LayoutNode::Split { first, second, .. } => {
            split_at_panel(first, target, panel.clone(), axis, ratio, placement)
                || split_at_panel(second, target, panel, axis, ratio, placement)
        }
        LayoutNode::Panel { id } if id == target => {
            let existing = std::mem::replace(node, LayoutNode::Empty);
            let (first, second) = match placement {
                SplitPlacement::First => (LayoutNode::Panel { id: panel }, existing),
                SplitPlacement::Second => (existing, LayoutNode::Panel { id: panel }),
            };
            *node = LayoutNode::split(axis, ratio, first, second);
            true
        }
        LayoutNode::Tabs { items, .. } if items.contains(target) => {
            let existing = std::mem::replace(node, LayoutNode::Empty);
            let (first, second) = match placement {
                SplitPlacement::First => (LayoutNode::Panel { id: panel }, existing),
                SplitPlacement::Second => (existing, LayoutNode::Panel { id: panel }),
            };
            *node = LayoutNode::split(axis, ratio, first, second);
            true
        }
        LayoutNode::Tabs { .. }
        | LayoutNode::Panel { .. }
        | LayoutNode::Documents { .. }
        | LayoutNode::Empty => false,
    }
}

fn tab_with_panel(node: &mut LayoutNode, target: &PanelId, panel: PanelId) -> bool {
    match node {
        LayoutNode::Split { first, second, .. } => {
            tab_with_panel(first, target, panel.clone()) || tab_with_panel(second, target, panel)
        }
        LayoutNode::Tabs { items, active } if items.contains(target) => {
            items.push(panel.clone());
            *active = Some(panel);
            true
        }
        LayoutNode::Panel { id } if id == target => {
            let existing = std::mem::replace(node, LayoutNode::Empty);
            let LayoutNode::Panel { id: existing_id } = existing else {
                unreachable!("matched panel node was replaced")
            };
            *node = LayoutNode::Tabs {
                items: vec![existing_id, panel.clone()],
                active: Some(panel),
            };
            true
        }
        LayoutNode::Tabs { .. }
        | LayoutNode::Panel { .. }
        | LayoutNode::Documents { .. }
        | LayoutNode::Empty => false,
    }
}

fn detach_panel_from_node(node: &mut LayoutNode, panel: &PanelId) -> bool {
    match node {
        LayoutNode::Split { first, second, .. } => {
            detach_panel_from_node(first, panel) || detach_panel_from_node(second, panel)
        }
        LayoutNode::Tabs { items, active } => {
            let Some(index) = items.iter().position(|id| id == panel) else {
                return false;
            };
            items.remove(index);
            if active.as_ref() == Some(panel) {
                *active = items.get(index).or_else(|| items.last()).cloned();
            }
            true
        }
        LayoutNode::Panel { id } if id == panel => {
            *node = LayoutNode::Empty;
            true
        }
        LayoutNode::Panel { .. } | LayoutNode::Documents { .. } | LayoutNode::Empty => false,
    }
}

fn take_id(ids: &mut Vec<PanelId>, target: &PanelId) -> bool {
    let Some(index) = ids.iter().position(|id| id == target) else {
        return false;
    };
    ids.remove(index);
    true
}

fn take_panel(items: &mut Vec<FloatingPanel>, target: &PanelId) -> bool {
    let Some(index) = items.iter().position(|item| &item.id == target) else {
        return false;
    };
    items.remove(index);
    true
}

fn activate_panel(node: &mut LayoutNode, panel: &PanelId) -> bool {
    match node {
        LayoutNode::Split { first, second, .. } => {
            activate_panel(first, panel) || activate_panel(second, panel)
        }
        LayoutNode::Tabs { items, active } if items.contains(panel) => {
            *active = Some(panel.clone());
            true
        }
        LayoutNode::Panel { id } => id == panel,
        LayoutNode::Tabs { .. } | LayoutNode::Documents { .. } | LayoutNode::Empty => false,
    }
}

fn resize_split_for_panel(node: &mut LayoutNode, panel: &PanelId, ratio: f32) -> bool {
    let LayoutNode::Split {
        ratio: split_ratio,
        first,
        second,
        ..
    } = node
    else {
        return false;
    };

    if contains_panel(first, panel) || contains_panel(second, panel) {
        if resize_split_for_panel(first, panel, ratio)
            || resize_split_for_panel(second, panel, ratio)
        {
            return true;
        }
        *split_ratio = normalize_ratio(ratio);
        return true;
    }
    false
}

fn contains_panel(node: &LayoutNode, panel: &PanelId) -> bool {
    match node {
        LayoutNode::Split { first, second, .. } => {
            contains_panel(first, panel) || contains_panel(second, panel)
        }
        LayoutNode::Tabs { items, .. } => items.contains(panel),
        LayoutNode::Panel { id } => id == panel,
        LayoutNode::Documents { .. } | LayoutNode::Empty => false,
    }
}

fn detach_document(node: &mut LayoutNode, document: &DocumentId) -> bool {
    match node {
        LayoutNode::Split { first, second, .. } => {
            detach_document(first, document) || detach_document(second, document)
        }
        LayoutNode::Documents { items, active, .. } => {
            let Some(index) = items.iter().position(|item| item == document) else {
                return false;
            };
            items.remove(index);
            if active.as_ref() == Some(document) {
                *active = items.get(index).or_else(|| items.last()).cloned();
            }
            true
        }
        LayoutNode::Tabs { .. } | LayoutNode::Panel { .. } | LayoutNode::Empty => false,
    }
}

fn take_document(items: &mut Vec<FloatingDocument>, target: &DocumentId) -> bool {
    let Some(index) = items.iter().position(|item| &item.id == target) else {
        return false;
    };
    items.remove(index);
    true
}

fn insert_document(node: &mut LayoutNode, document: DocumentId) -> bool {
    match node {
        LayoutNode::Split { first, second, .. } => {
            insert_document(first, document.clone()) || insert_document(second, document)
        }
        LayoutNode::Documents { items, active, .. } => {
            items.push(document.clone());
            *active = Some(document);
            true
        }
        LayoutNode::Tabs { .. } | LayoutNode::Panel { .. } | LayoutNode::Empty => false,
    }
}

fn activate_document(node: &mut LayoutNode, document: &DocumentId) -> bool {
    match node {
        LayoutNode::Split { first, second, .. } => {
            activate_document(first, document) || activate_document(second, document)
        }
        LayoutNode::Documents { items, active, .. } if items.contains(document) => {
            *active = Some(document.clone());
            true
        }
        LayoutNode::Documents { .. }
        | LayoutNode::Tabs { .. }
        | LayoutNode::Panel { .. }
        | LayoutNode::Empty => false,
    }
}

fn clamp_bounds(
    bounds: WorkspaceBounds,
    work_area: WorkspaceBounds,
    min_visible: f32,
) -> WorkspaceBounds {
    let visible_width = min_visible.min(bounds.width).min(work_area.width);
    let visible_height = min_visible.min(bounds.height).min(work_area.height);
    let lower_x = work_area.x + visible_width - bounds.width;
    let upper_x = work_area.x + work_area.width - visible_width;
    let lower_y = work_area.y + visible_height - bounds.height;
    let upper_y = work_area.y + work_area.height - visible_height;
    WorkspaceBounds {
        x: bounds.x.clamp(lower_x.min(upper_x), lower_x.max(upper_x)),
        y: bounds.y.clamp(lower_y.min(upper_y), lower_y.max(upper_y)),
        width: bounds.width,
        height: bounds.height,
    }
}

fn next_z_order<T>(items: &[T]) -> u32
where
    T: ZOrdered,
{
    items
        .iter()
        .map(ZOrdered::z_order)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

fn normalize_z_order<T>(items: &mut [T])
where
    T: ZOrdered,
{
    let mut order = (0..items.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| items[index].z_order());
    for (rank, index) in order.into_iter().enumerate() {
        items[index].set_z_order(u32::try_from(rank + 1).unwrap_or(u32::MAX));
    }
}

fn bring_index_to_front<T>(items: &mut [T], index: usize)
where
    T: ZOrdered,
{
    if index >= items.len() {
        return;
    }
    let mut order = (0..items.len())
        .filter(|&item| item != index)
        .collect::<Vec<_>>();
    order.sort_by_key(|&item| items[item].z_order());
    order.push(index);
    for (rank, item) in order.into_iter().enumerate() {
        items[item].set_z_order(u32::try_from(rank + 1).unwrap_or(u32::MAX));
    }
}

trait ZOrdered {
    fn z_order(&self) -> u32;
    fn set_z_order(&mut self, z_order: u32);
}

impl ZOrdered for FloatingPanel {
    fn z_order(&self) -> u32 {
        self.z_order
    }

    fn set_z_order(&mut self, z_order: u32) {
        self.z_order = z_order;
    }
}

impl ZOrdered for FloatingDocument {
    fn z_order(&self) -> u32 {
        self.z_order
    }

    fn set_z_order(&mut self, z_order: u32) {
        self.z_order = z_order;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DockAxis, DocumentId, FloatingPanel, LayoutNode, PanelId, SplitPlacement, WorkspaceBounds,
        WorkspaceError, WorkspaceLayout,
    };

    fn panel(id: &str) -> PanelId {
        PanelId::from(id)
    }

    fn document(id: &str) -> DocumentId {
        DocumentId::from(id)
    }

    fn bounds() -> WorkspaceBounds {
        WorkspaceBounds::new(10.0, 20.0, 300.0, 200.0).expect("valid test bounds")
    }

    #[test]
    fn moves_panels_between_tabs_floating_and_hidden_without_duplicates() {
        let left = panel("left");
        let right = panel("right");
        let mut layout = WorkspaceLayout::new(LayoutNode::Panel { id: left.clone() })
            .expect("initial layout is valid");

        layout
            .move_to_tabs(&left, right.clone())
            .expect("can add a registered panel");
        layout
            .float_panel(&right, bounds())
            .expect("can float panel");
        layout.hide_panel(&right).expect("can hide floating panel");
        layout
            .show_panel(&right, &left)
            .expect("can show hidden panel");

        assert!(
            matches!(layout.root(), LayoutNode::Tabs { items, active: Some(active) } if items == &vec![left, right.clone()] && active == &right)
        );
        assert!(layout.floating().is_empty());
        assert!(layout.hidden().is_empty());
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn splitting_a_tab_group_preserves_its_existing_tabs() {
        let first = panel("first");
        let second = panel("second");
        let third = panel("third");
        let mut layout = WorkspaceLayout::new(LayoutNode::tabs(vec![first.clone(), second]))
            .expect("initial layout is valid");

        layout
            .split_panel(
                &first,
                third.clone(),
                DockAxis::Horizontal,
                0.01,
                SplitPlacement::Second,
            )
            .expect("can split a tab group");

        assert!(
            matches!(layout.root(), LayoutNode::Split { ratio, first, second, .. } if (*ratio - 0.1).abs() < f32::EPSILON && matches!(first.as_ref(), LayoutNode::Tabs { .. }) && matches!(second.as_ref(), LayoutNode::Panel { id } if id == &third))
        );
    }

    #[test]
    fn failed_operation_keeps_the_original_layout() {
        let source = panel("source");
        let missing_target = panel("missing");
        let mut layout = WorkspaceLayout::new(LayoutNode::Panel { id: source.clone() })
            .expect("initial layout is valid");
        let original = layout.clone();

        assert_eq!(
            layout.move_to_tabs(&missing_target, source),
            Err(WorkspaceError::PanelNotFound(missing_target))
        );
        assert_eq!(layout, original);
    }

    #[test]
    fn resizing_a_split_clamps_the_ratio_and_preserves_the_layout() {
        let left = panel("left");
        let right = panel("right");
        let mut layout = WorkspaceLayout::new(LayoutNode::split(
            DockAxis::Horizontal,
            0.5,
            LayoutNode::Panel { id: left.clone() },
            LayoutNode::Panel { id: right },
        ))
        .expect("initial layout is valid");

        layout
            .resize_split_for_panel(&left, 2.0)
            .expect("docked panel identifies its split");

        assert!(
            matches!(layout.root(), LayoutNode::Split { ratio, .. } if (*ratio - 0.9).abs() < f32::EPSILON)
        );
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn rejects_duplicate_panel_ids_across_docked_and_floating_locations() {
        let id = panel("inspector");
        let mut layout = WorkspaceLayout::new(LayoutNode::Panel { id: id.clone() })
            .expect("initial layout is valid");
        layout.floating.push(FloatingPanel {
            id: id.clone(),
            bounds: bounds(),
            visible: true,
            z_order: 1,
        });

        assert_eq!(layout.validate(), Err(WorkspaceError::DuplicatePanel(id)));
    }

    #[test]
    fn freeform_documents_round_trip_to_the_document_area() {
        let first = document("first");
        let second = document("second");
        let mut layout =
            WorkspaceLayout::new(LayoutNode::documents(vec![first.clone(), second.clone()]))
                .expect("initial layout is valid");

        layout
            .float_document(&second, bounds())
            .expect("can float document");
        layout
            .dock_document(&second)
            .expect("can return document to central area");

        assert!(
            matches!(layout.root(), LayoutNode::Documents { items, active: Some(active), .. } if items == &vec![first, second.clone()] && active == &second)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn json_restore_normalizes_ratios_and_preserves_unknown_ids() {
        let layout = WorkspaceLayout::new(LayoutNode::split(
            DockAxis::Vertical,
            0.5,
            LayoutNode::Panel { id: panel("known") },
            LayoutNode::Panel {
                id: panel("unknown"),
            },
        ))
        .expect("initial layout is valid");
        let json = layout.to_json().expect("JSON encoding succeeds");

        let restored = WorkspaceLayout::from_json(&json).expect("JSON decoding succeeds");

        assert_eq!(restored, layout);
        assert!(json.contains("unknown"));
    }

    #[test]
    fn floating_panels_move_resize_and_raise_with_dense_z_order() {
        let first = panel("first");
        let second = panel("second");
        let third = panel("third");
        let mut layout = WorkspaceLayout::new(LayoutNode::tabs(vec![
            first.clone(),
            second.clone(),
            third.clone(),
        ]))
        .expect("initial layout is valid");

        for id in [&first, &second, &third] {
            layout
                .float_panel(id, bounds())
                .expect("can float a docked panel");
        }
        layout
            .bring_panel_to_front(&first)
            .expect("can raise a floating panel");

        let z_order = |id: &PanelId| {
            layout
                .floating()
                .iter()
                .find(|item| &item.id == id)
                .expect("panel is floating")
                .z_order
        };
        assert_eq!(z_order(&second), 1);
        assert_eq!(z_order(&third), 2);
        assert_eq!(z_order(&first), 3);

        layout
            .move_floating_panel(&first, 40.0, 50.0)
            .expect("can move a floating panel");
        let moved = layout
            .floating()
            .iter()
            .find(|item| item.id == first)
            .expect("panel is floating");
        assert_eq!(
            moved.bounds,
            WorkspaceBounds::new(40.0, 50.0, 300.0, 200.0).unwrap()
        );

        layout
            .resize_floating_panel(&first, 500.0, 250.0)
            .expect("can resize a floating panel");
        let resized = layout
            .floating()
            .iter()
            .find(|item| item.id == first)
            .expect("panel is floating");
        assert_eq!(
            resized.bounds,
            WorkspaceBounds::new(40.0, 50.0, 500.0, 250.0).unwrap()
        );
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn freeform_documents_move_resize_and_raise_with_dense_z_order() {
        let first = document("first");
        let second = document("second");
        let mut layout =
            WorkspaceLayout::new(LayoutNode::documents(vec![first.clone(), second.clone()]))
                .expect("initial layout is valid");

        layout
            .float_document(&first, bounds())
            .expect("can float a document");
        layout
            .float_document(&second, bounds())
            .expect("can float a document");
        layout
            .bring_document_to_front(&first)
            .expect("can raise a free-form document");

        let z_order = |id: &DocumentId| {
            layout
                .freeform_documents()
                .iter()
                .find(|item| &item.id == id)
                .expect("document is free-form")
                .z_order
        };
        assert_eq!(z_order(&second), 1);
        assert_eq!(z_order(&first), 2);

        layout
            .move_floating_document(&first, 70.0, 80.0)
            .expect("can move a free-form document");
        layout
            .resize_floating_document(&first, 640.0, 480.0)
            .expect("can resize a free-form document");
        let moved = layout
            .freeform_documents()
            .iter()
            .find(|item| item.id == first)
            .expect("document is free-form");
        assert_eq!(
            moved.bounds,
            WorkspaceBounds::new(70.0, 80.0, 640.0, 480.0).unwrap()
        );
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn floating_operations_reject_items_that_are_not_floating() {
        let docked = panel("docked");
        let docked_document = document("docked-document");
        let mut layout = WorkspaceLayout::new(LayoutNode::split(
            DockAxis::Horizontal,
            0.5,
            LayoutNode::Panel { id: docked.clone() },
            LayoutNode::documents(vec![docked_document.clone()]),
        ))
        .expect("initial layout is valid");

        assert_eq!(
            layout.move_floating_panel(&docked, 0.0, 0.0),
            Err(WorkspaceError::PanelNotFloating(docked.clone()))
        );
        assert_eq!(
            layout.bring_panel_to_front(&docked),
            Err(WorkspaceError::PanelNotFloating(docked))
        );
        assert_eq!(
            layout.move_floating_document(&docked_document, 0.0, 0.0),
            Err(WorkspaceError::DocumentNotFloating(docked_document.clone()))
        );
        assert_eq!(
            layout.bring_document_to_front(&docked_document),
            Err(WorkspaceError::DocumentNotFloating(docked_document))
        );
    }

    #[test]
    fn removing_panels_collapses_the_tree_and_selects_a_neighbor() {
        let first = panel("first");
        let second = panel("second");
        let third = panel("third");
        let docked = panel("docked");
        let mut layout = WorkspaceLayout::new(LayoutNode::split(
            DockAxis::Horizontal,
            0.5,
            LayoutNode::tabs(vec![first.clone(), second.clone(), third.clone()]),
            LayoutNode::Panel { id: docked.clone() },
        ))
        .expect("initial layout is valid");
        layout
            .activate_panel(&second)
            .expect("second tab is docked");

        layout
            .remove_panel(&second)
            .expect("can remove a docked panel");
        assert!(
            matches!(layout.root(), LayoutNode::Split { first: branch, .. } if matches!(branch.as_ref(), LayoutNode::Tabs { items, active: Some(active) } if items == &vec![first.clone(), third.clone()] && active == &third))
        );

        layout
            .remove_panel(&third)
            .expect("can remove another docked panel");
        assert!(
            matches!(layout.root(), LayoutNode::Split { first: branch, .. } if matches!(branch.as_ref(), LayoutNode::Panel { id } if id == &first))
        );

        layout
            .remove_panel(&first)
            .expect("can remove the last panel in a split branch");
        assert_eq!(layout.root(), &LayoutNode::Panel { id: docked });
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn removing_panels_frees_floating_and_hidden_storage() {
        let floating = panel("floating");
        let hidden = panel("hidden");
        let mut layout =
            WorkspaceLayout::new(LayoutNode::tabs(vec![floating.clone(), hidden.clone()]))
                .expect("initial layout is valid");
        layout
            .float_panel(&floating, bounds())
            .expect("can float a panel");
        layout.hide_panel(&hidden).expect("can hide a panel");

        layout
            .remove_panel(&floating)
            .expect("can remove a floating panel");
        layout
            .remove_panel(&hidden)
            .expect("can remove a hidden panel");

        assert_eq!(layout.root(), &LayoutNode::Empty);
        assert!(layout.floating().is_empty());
        assert!(layout.hidden().is_empty());
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn closing_documents_frees_both_areas_and_selects_a_neighbor() {
        let first = document("first");
        let second = document("second");
        let third = document("third");
        let mut layout = WorkspaceLayout::new(LayoutNode::documents(vec![
            first.clone(),
            second.clone(),
            third.clone(),
        ]))
        .expect("initial layout is valid");
        layout
            .activate_document(&second)
            .expect("second document is docked");

        layout
            .close_document(&second)
            .expect("can close the active document");
        assert!(
            matches!(layout.root(), LayoutNode::Documents { items, active: Some(active), .. } if items == &vec![first.clone(), third.clone()] && active == &third)
        );

        layout
            .close_document(&third)
            .expect("can close the last document");
        assert!(
            matches!(layout.root(), LayoutNode::Documents { items, active: Some(active), .. } if items == &vec![first.clone()] && active == &first)
        );

        layout
            .close_document(&first)
            .expect("can close the final document");
        assert!(
            matches!(layout.root(), LayoutNode::Documents { items, active: None, .. } if items.is_empty())
        );

        let freeform = document("freeform");
        layout
            .float_document(&freeform, bounds())
            .expect_err("no docked document remains to float");
        let mut layout = WorkspaceLayout::new(LayoutNode::documents(vec![freeform.clone()]))
            .expect("initial layout is valid");
        layout
            .float_document(&freeform, bounds())
            .expect("can float a document");
        layout
            .close_document(&freeform)
            .expect("can close a free-form document");
        assert!(layout.freeform_documents().is_empty());
        assert!(
            matches!(layout.root(), LayoutNode::Documents { items, active: None, .. } if items.is_empty())
        );
    }

    #[test]
    fn normalization_collapses_empty_tabs_and_empty_split_branches() {
        let only = panel("only");
        let layout = WorkspaceLayout::new(LayoutNode::tabs(Vec::new()))
            .expect("an empty tab group becomes an empty root");
        assert_eq!(layout.root(), &LayoutNode::Empty);

        let layout = WorkspaceLayout::new(LayoutNode::tabs(vec![only.clone()]))
            .expect("a one-item tab group becomes a panel");
        assert_eq!(layout.root(), &LayoutNode::Panel { id: only.clone() });

        let layout = WorkspaceLayout::new(LayoutNode::split(
            DockAxis::Vertical,
            0.5,
            LayoutNode::Empty,
            LayoutNode::Panel { id: only.clone() },
        ))
        .expect("an empty split branch collapses");
        assert_eq!(layout.root(), &LayoutNode::Panel { id: only });
    }

    #[test]
    fn clamping_floating_bounds_keeps_a_minimum_visible_extent() {
        let floating = panel("floating");
        let freeform = document("freeform");
        let mut layout = WorkspaceLayout::new(LayoutNode::split(
            DockAxis::Horizontal,
            0.5,
            LayoutNode::Panel {
                id: floating.clone(),
            },
            LayoutNode::documents(vec![freeform.clone()]),
        ))
        .expect("initial layout is valid");
        layout
            .float_panel(
                &floating,
                WorkspaceBounds::new(-900.0, -900.0, 300.0, 200.0).unwrap(),
            )
            .expect("can float a panel");
        layout
            .float_document(
                &freeform,
                WorkspaceBounds::new(5_000.0, 5_000.0, 5_000.0, 5_000.0).unwrap(),
            )
            .expect("can float a document");

        layout
            .clamp_floating_bounds(WorkspaceBounds::new(0.0, 0.0, 1_000.0, 800.0).unwrap())
            .expect("can clamp floating bounds");

        let panel_bounds = layout.floating()[0].bounds;
        assert_eq!(
            panel_bounds,
            WorkspaceBounds::new(-268.0, -168.0, 300.0, 200.0).unwrap()
        );
        assert!((panel_bounds.x() + panel_bounds.width() - 32.0).abs() < f32::EPSILON);
        assert!((panel_bounds.y() + panel_bounds.height() - 32.0).abs() < f32::EPSILON);

        let document_bounds = layout.freeform_documents()[0].bounds;
        assert_eq!(
            document_bounds,
            WorkspaceBounds::new(968.0, 768.0, 5_000.0, 5_000.0).unwrap()
        );
        assert!((document_bounds.x() - 968.0).abs() < f32::EPSILON);
        assert!((document_bounds.y() - 768.0).abs() < f32::EPSILON);
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn clamping_floating_bounds_rejects_an_invalid_visible_extent() {
        let mut layout = WorkspaceLayout::new(LayoutNode::Empty).expect("empty layout is valid");
        assert_eq!(
            layout.clamp_floating_bounds_with_min_visible(
                WorkspaceBounds::new(0.0, 0.0, 100.0, 100.0).unwrap(),
                -1.0,
            ),
            Err(WorkspaceError::InvalidVisibleExtent)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn json_restore_uses_the_migration_hook_for_version_checks() {
        use super::WorkspaceSerializationError;

        let layout =
            WorkspaceLayout::new(LayoutNode::Panel { id: panel("panel") }).expect("valid layout");
        let mut value: serde_json::Value =
            serde_json::from_str(&layout.to_json().expect("JSON encoding succeeds"))
                .expect("valid JSON value");

        value["version"] = serde_json::json!(0);
        let old = serde_json::to_string(&value).expect("JSON encoding succeeds");
        assert!(matches!(
            WorkspaceLayout::from_json(&old),
            Err(WorkspaceSerializationError::Workspace(
                WorkspaceError::MigrationRequired(0)
            ))
        ));

        value["version"] = serde_json::json!(2);
        let future = serde_json::to_string(&value).expect("JSON encoding succeeds");
        assert!(matches!(
            WorkspaceLayout::from_json(&future),
            Err(WorkspaceSerializationError::Workspace(
                WorkspaceError::UnsupportedVersion(2)
            ))
        ));
    }
}
