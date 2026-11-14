//! Data-driven virtualized list and table controls.
//!
//! Unlike [`crate::ListView`], these controls own their row data. Only the rows
//! intersecting the viewport are shaped and painted, row visuals are cached by
//! stable key, and keyboard navigation keeps the focused row visible.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use drawen_core::{
    Constraints, FocusChanged, InputEvent, Key, Modifiers, NamedKey, Point, PointerButton, Rect,
    Size, WheelDelta,
};
use drawen_render::{PaintContext, TextLayout, TextStyle};
use drawen_ui::{
    Accessibility, AccessibilityAction, AccessibilityActions, AccessibilityRole,
    AccessibilityState, DssStyle, EventContext, EventPhase, EventStatus, LayoutContext,
    LayoutResult, Signal, StyleTarget, UiEnvironment, Widget,
};

const DEFAULT_WIDTH: f32 = 240.0;
const DEFAULT_HEIGHT: f32 = 180.0;
const ROW_HEIGHT: f32 = 28.0;
const HEADER_HEIGHT: f32 = 30.0;
const HORIZONTAL_PADDING: f32 = 10.0;
const OVERSCAN: usize = 1;
const CACHE_LIMIT: usize = 2048;

/// Read-only data backing a [`VirtualList`].
pub trait VirtualDataSource {
    /// Returns the total number of rows.
    fn row_count(&self) -> usize;

    /// Returns a stable key for `row`.
    ///
    /// Keys must remain unchanged while the row's identity is unchanged. The
    /// control uses them to reuse shaped row visuals across frames.
    fn row_key(&self, row: usize) -> u64;

    /// Returns the row's display text.
    fn row_text(&self, row: usize) -> String;

    /// Returns an optional accessible name for the whole list.
    fn label(&self) -> Option<String> {
        None
    }
}

/// Read-only data backing a [`VirtualTable`].
pub trait VirtualTableSource {
    /// Returns the total number of rows.
    fn row_count(&self) -> usize;

    /// Returns a stable key for `row`.
    fn row_key(&self, row: usize) -> u64;

    /// Returns the total number of columns.
    fn column_count(&self) -> usize;

    /// Returns the column's header text.
    fn column_title(&self, column: usize) -> String;

    /// Returns the preferred column width in logical pixels.
    fn column_width(&self, column: usize) -> f32;

    /// Returns the text displayed in `row` and `column`.
    fn cell_text(&self, row: usize, column: usize) -> String;

    /// Returns an optional accessible name for the whole table.
    fn label(&self) -> Option<String> {
        None
    }
}

/// How a list or table interprets selection input.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SelectionMode {
    /// Exactly zero or one row is selected.
    #[default]
    Single,
    /// Any number of rows can be selected with Control or Space.
    Multiple,
}

/// Selection and keyboard focus state shared by virtual list and table views.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionModel {
    mode: SelectionMode,
    selected: BTreeSet<usize>,
    focused: Option<usize>,
    anchor: Option<usize>,
}

impl SelectionModel {
    /// Creates an empty single-selection model.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the selection mode.
    #[must_use]
    pub const fn mode(&self) -> SelectionMode {
        self.mode
    }

    /// Changes the selection mode, discarding extra selections when narrowing.
    pub fn set_mode(&mut self, mode: SelectionMode) {
        self.mode = mode;
        if mode == SelectionMode::Single && self.selected.len() > 1 {
            let retain = self
                .focused
                .or_else(|| self.selected.iter().next().copied());
            self.selected.clear();
            self.selected.extend(retain);
        }
    }

    /// Returns selected row indices in ascending order.
    #[must_use]
    pub fn selected(&self) -> &BTreeSet<usize> {
        &self.selected
    }

    /// Returns the row that owns keyboard focus.
    #[must_use]
    pub const fn focused(&self) -> Option<usize> {
        self.focused
    }

    /// Returns whether `row` is selected.
    #[must_use]
    pub fn contains(&self, row: usize) -> bool {
        self.selected.contains(&row)
    }

    /// Clears selection and focus.
    pub fn clear(&mut self) {
        self.selected.clear();
        self.focused = None;
        self.anchor = None;
    }

    /// Removes rows that no longer exist.
    pub fn retain(&mut self, count: usize) {
        self.selected.retain(|row| *row < count);
        if self.focused.is_some_and(|row| row >= count) {
            self.focused = count.checked_sub(1);
        }
        if self.anchor.is_some_and(|row| row >= count) {
            self.anchor = self.focused;
        }
    }

    /// Selects only `row` and moves keyboard focus to it.
    pub fn select_only(&mut self, row: usize) -> bool {
        let changed =
            self.focused != Some(row) || self.selected.len() != 1 || !self.selected.contains(&row);
        self.selected.clear();
        self.selected.insert(row);
        self.focused = Some(row);
        self.anchor = Some(row);
        changed
    }

    /// Extends the current range to `row`, preserving the range anchor.
    pub fn extend_to(&mut self, row: usize) -> bool {
        let anchor = self.anchor.unwrap_or(row);
        let range = anchor.min(row)..=anchor.max(row);
        let changed = self.focused != Some(row) || self.selected.len() != range.clone().count();
        self.selected.clear();
        self.selected.extend(range);
        self.focused = Some(row);
        changed
    }

    /// Toggles `row` without discarding the rest of the selection.
    pub fn toggle(&mut self, row: usize) -> bool {
        if self.selected.remove(&row) {
            if self.focused == Some(row) {
                self.focused = self.selected.iter().next_back().copied();
            }
        } else {
            self.selected.insert(row);
            self.focused = Some(row);
            self.anchor = Some(row);
        }
        true
    }

    /// Selects every row in `0..count`.
    pub fn select_all(&mut self, count: usize) -> bool {
        if count == 0 {
            return false;
        }
        self.selected = (0..count).collect();
        self.focused.get_or_insert(0);
        self.anchor = Some(self.focused.unwrap_or(0));
        true
    }

    /// Moves focus to `row`, selecting it in single-selection mode.
    pub fn focus(&mut self, row: usize, extend: bool) -> bool {
        if extend || self.mode == SelectionMode::Multiple {
            self.extend_to(row)
        } else {
            self.select_only(row)
        }
    }
}

/// A virtualized, selectable list backed by a [`VirtualDataSource`].
pub struct VirtualList<D> {
    data: D,
    row_height: f32,
    offset: f32,
    viewport: Size,
    visible: Vec<VisibleRow>,
    cache: HashMap<u64, RowVisual>,
    selection: SelectionModel,
    selection_changed: Signal<Vec<usize>>,
    activated: Signal<usize>,
    keyboard_focus: bool,
    dragging: bool,
}

struct VisibleRow {
    index: usize,
    key: u64,
    layout: Arc<TextLayout>,
    y: f32,
}

struct RowVisual {
    text: String,
    layout: Arc<TextLayout>,
}

impl<D: VirtualDataSource> VirtualList<D> {
    /// Creates a list backed by `data` with fixed row height.
    ///
    /// # Panics
    ///
    /// Panics when `row_height` is not finite and positive.
    #[must_use]
    pub fn new(data: D, row_height: f32) -> Self {
        assert!(
            row_height.is_finite() && row_height > 0.0,
            "virtual list row height must be finite and positive"
        );
        Self {
            data,
            row_height,
            offset: 0.0,
            viewport: Size::ZERO,
            visible: Vec::new(),
            cache: HashMap::new(),
            selection: SelectionModel::new(),
            selection_changed: Signal::new(),
            activated: Signal::new(),
            keyboard_focus: false,
            dragging: false,
        }
    }

    /// Returns the backing data.
    #[must_use]
    pub const fn data(&self) -> &D {
        &self.data
    }

    /// Returns mutable access to the backing data and invalidates row visuals.
    pub fn data_mut(&mut self) -> &mut D {
        self.cache.clear();
        &mut self.data
    }

    /// Returns the selection model.
    #[must_use]
    pub const fn selection(&self) -> &SelectionModel {
        &self.selection
    }

    /// Returns mutable access to the selection model.
    pub fn selection_mut(&mut self) -> &mut SelectionModel {
        &mut self.selection
    }

    /// Emits the sorted selected row indices whenever selection changes.
    #[must_use]
    pub fn selection_changed(&self) -> Signal<Vec<usize>> {
        self.selection_changed.clone()
    }

    /// Emits a row index when the user activates it with Enter or a double click.
    #[must_use]
    pub fn activated(&self) -> Signal<usize> {
        self.activated.clone()
    }

    /// Returns the current vertical scroll offset.
    #[must_use]
    pub const fn offset(&self) -> f32 {
        self.offset
    }

    /// Returns the number of rows measured for the current viewport.
    #[must_use]
    pub fn visible_row_count(&self) -> usize {
        self.visible.len()
    }

    /// Returns the number of shaped rows retained for key-based reuse.
    #[must_use]
    pub fn cached_row_count(&self) -> usize {
        self.cache.len()
    }

    /// Sets the vertical scroll offset, clamped to the content bounds.
    pub fn set_offset(&mut self, offset: f32) {
        self.offset = offset.clamp(0.0, self.max_offset());
    }

    fn max_offset(&self) -> f32 {
        let content = row_extent(self.data.row_count(), self.row_height);
        (content - self.viewport.height()).max(0.0)
    }

    fn announce_selection(&self) {
        self.selection_changed.emit(
            &self
                .selection
                .selected()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
        );
    }

    fn set_selection(&mut self, context: &mut EventContext<'_>, changed: bool) {
        if changed {
            self.announce_selection();
            context.request_redraw();
        }
    }

    fn ensure_visible(&mut self, row: usize) {
        let top = index_offset(row, self.row_height);
        let bottom = top + self.row_height;
        let height = self.viewport.height().max(self.row_height);
        if top < self.offset {
            self.offset = top;
        } else if bottom > self.offset + height {
            self.offset = bottom - height;
        }
        self.set_offset(self.offset);
    }

    fn row_at(&self, position: Point) -> Option<usize> {
        let y = position.y + self.offset;
        if y < 0.0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = (y / self.row_height) as usize;
        (row < self.data.row_count()).then_some(row)
    }

    fn move_focus(&mut self, context: &mut EventContext<'_>, target: Option<usize>, extend: bool) {
        let count = self.data.row_count();
        if count == 0 {
            return;
        }
        let row = target.unwrap_or(0).min(count - 1);
        let changed = self.selection.focus(row, extend);
        self.ensure_visible(row);
        self.set_selection(context, changed);
        context.request_layout();
    }

    fn handle_key(
        &mut self,
        context: &mut EventContext<'_>,
        key: &drawen_core::KeyEvent,
    ) -> EventStatus {
        let extend = key.modifiers.contains(Modifiers::SHIFT);
        let command =
            key.modifiers.contains(Modifiers::CONTROL) || key.modifiers.contains(Modifiers::META);
        let count = self.data.row_count();
        let focused = self.selection.focused();
        match &key.key {
            Key::Named(NamedKey::ArrowUp) => {
                self.move_focus(
                    context,
                    Some(focused.map_or(0, |row| row.saturating_sub(1))),
                    extend,
                );
                EventStatus::Handled
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.move_focus(
                    context,
                    Some(focused.map_or(0, |row| (row + 1).min(count.saturating_sub(1)))),
                    extend,
                );
                EventStatus::Handled
            }
            Key::Named(NamedKey::Home) => {
                self.move_focus(context, Some(0), extend);
                EventStatus::Handled
            }
            Key::Named(NamedKey::End) => {
                self.move_focus(context, Some(count.saturating_sub(1)), extend);
                EventStatus::Handled
            }
            Key::Named(NamedKey::PageUp | NamedKey::PageDown) => {
                let page = (self.viewport.height() / self.row_height).floor().max(1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let page = page as usize;
                let row = if matches!(key.key, Key::Named(NamedKey::PageUp)) {
                    focused.map_or(0, |row| row.saturating_sub(page))
                } else {
                    focused.map_or(0, |row| (row + page).min(count.saturating_sub(1)))
                };
                self.move_focus(context, Some(row), extend);
                EventStatus::Handled
            }
            Key::Named(NamedKey::Space) => {
                if let Some(row) = focused {
                    let changed = self.selection.toggle(row);
                    self.set_selection(context, changed);
                }
                EventStatus::Handled
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(row) = focused {
                    self.activated.emit(&row);
                }
                EventStatus::Handled
            }
            Key::Character(character) if command && character.eq_ignore_ascii_case("a") => {
                if self.selection.select_all(count) {
                    self.announce_selection();
                    context.request_redraw();
                }
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }
}

impl<D: VirtualDataSource> Widget for VirtualList<D> {
    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("virtual-list")
    }

    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let environment = context.environment();
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            DEFAULT_WIDTH
        };
        let height = if max.height().is_finite() {
            max.height()
        } else {
            DEFAULT_HEIGHT
        };
        self.viewport = constraints.constrain(size(width, height));
        self.selection.retain(self.data.row_count());
        self.set_offset(self.offset);

        let font = environment.font_for(&style);
        let text_style =
            TextStyle::new_with_font(font, style.foreground_or(environment.theme().foreground()));
        let first = self.first_visible();
        let last = self.last_visible();
        self.visible.clear();
        for row in first..last {
            let key = self.data.row_key(row);
            let text = self.data.row_text(row);
            let cached = self
                .cache
                .get(&key)
                .filter(|visual| visual.text == text)
                .map(|visual| Arc::clone(&visual.layout));
            let layout = if let Some(layout) = cached {
                layout
            } else {
                let layout = Arc::new(environment.text_shaper().layout(&text, &text_style, None));
                self.cache.insert(
                    key,
                    RowVisual {
                        text,
                        layout: Arc::clone(&layout),
                    },
                );
                layout
            };
            self.visible.push(VisibleRow {
                index: row,
                key,
                layout,
                y: index_offset(row, self.row_height) - self.offset,
            });
        }
        if self.cache.len() > CACHE_LIMIT {
            let visible: BTreeSet<u64> = self.visible.iter().map(|row| row.key).collect();
            self.cache.retain(|key, _| visible.contains(key));
        }
        LayoutResult::new(self.viewport)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &DssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &DssStyle,
    ) {
        let theme = environment.theme();
        let foreground = style.foreground_or(theme.foreground());
        let accent = style.accent_or(theme.accent());
        context.fill_rounded_rect(
            bounds,
            style.corner_radius_or(theme.corner_radius()),
            style.background_or(theme.background()),
        );
        let width = bounds.size.width();
        for row in &self.visible {
            let row_bounds = Rect::new(
                Point::new(bounds.origin.x, bounds.origin.y + row.y),
                size(width, self.row_height),
            );
            if self.selection.contains(row.index) {
                context.fill_rect(row_bounds, accent.with_alpha(0.22));
            } else if self.selection.focused() == Some(row.index) && self.keyboard_focus {
                context.stroke_rect(row_bounds, 1.0, accent);
            }
            let layout = (*row.layout).clone().with_color(foreground);
            context.with_clip(row_bounds.inset(default_insets()), |context| {
                context.draw_text(
                    layout,
                    Point::new(
                        row_bounds.origin.x + HORIZONTAL_PADDING,
                        row_bounds.origin.y + (self.row_height - row.layout.size().height()) / 2.0,
                    ),
                );
            });
        }
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) => {
                if pointer.button != Some(PointerButton::Primary) {
                    return EventStatus::Ignored;
                }
                let Some(row) = self.row_at(pointer.position) else {
                    return EventStatus::Ignored;
                };
                context.request_focus();
                context.capture_pointer(pointer.pointer_id);
                self.dragging = true;
                let changed = if pointer.modifiers.contains(Modifiers::CONTROL) {
                    self.selection.toggle(row)
                } else {
                    self.selection
                        .focus(row, pointer.modifiers.contains(Modifiers::SHIFT))
                };
                self.set_selection(context, changed);
                EventStatus::Handled
            }
            InputEvent::PointerMove(pointer) => {
                if !self.dragging || !pointer.buttons.contains_button(PointerButton::Primary) {
                    return EventStatus::Ignored;
                }
                if let Some(row) = self.row_at(pointer.position) {
                    let changed = self.selection.extend_to(row);
                    self.set_selection(context, changed);
                }
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) => {
                if !self.dragging {
                    return EventStatus::Ignored;
                }
                self.dragging = false;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::PointerCancel(pointer) => {
                self.dragging = false;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::Wheel(wheel) => {
                let delta = match wheel.delta {
                    WheelDelta::Pixels(delta) => delta.y,
                    WheelDelta::Lines(delta) => delta.y * self.row_height * 3.0,
                };
                let old = self.offset;
                self.set_offset(self.offset - delta);
                if (self.offset - old).abs() <= f32::EPSILON {
                    return EventStatus::Ignored;
                }
                context.request_layout();
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) => self.handle_key(context, key),
            InputEvent::FocusChanged(FocusChanged { focused }) => {
                self.keyboard_focus = *focused == Some(context.current_target());
                context.request_redraw();
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let count = self.data.row_count();
        let selected = self.selection.selected().len();
        let mut accessibility = Accessibility::new(AccessibilityRole::List)
            .with_value(format!("{selected} of {count} selected"))
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(AccessibilityActions::FOCUS | AccessibilityActions::SELECT);
        if self.selection.mode() == SelectionMode::Multiple {
            accessibility = accessibility.with_states(AccessibilityState::MULTISELECTABLE);
        }
        if let Some(label) = self.data.label() {
            accessibility = accessibility.with_label(label);
        }
        accessibility
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        match action {
            AccessibilityAction::Select => {
                let row = self
                    .selection
                    .focused()
                    .or_else(|| (self.data.row_count() > 0).then_some(0));
                row.is_some_and(|row| self.selection.select_only(row))
            }
            _ => false,
        }
    }
}

impl<D: VirtualDataSource> VirtualList<D> {
    fn first_visible(&self) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = (self.offset / self.row_height) as usize;
        row.saturating_sub(OVERSCAN)
    }

    fn last_visible(&self) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = ((self.offset + self.viewport.height()) / self.row_height).ceil() as usize;
        (row + OVERSCAN + 1).min(self.data.row_count())
    }
}

/// A virtualized, selectable table backed by a [`VirtualTableSource`].
pub struct VirtualTable<D> {
    data: D,
    columns: Vec<Column>,
    row_height: f32,
    header_height: f32,
    offset: f32,
    viewport: Size,
    visible: Vec<VisibleTableRow>,
    cache: HashMap<u64, TableVisual>,
    selection: SelectionModel,
    focused_column: usize,
    selection_changed: Signal<Vec<usize>>,
    activated: Signal<(usize, usize)>,
    keyboard_focus: bool,
}

/// A table column's display metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    title: String,
    width: f32,
}

impl Column {
    /// Creates a column with a title and preferred logical width.
    ///
    /// # Panics
    ///
    /// Panics when `width` is not finite and positive.
    #[must_use]
    pub fn new(title: impl Into<String>, width: f32) -> Self {
        assert!(
            width.is_finite() && width > 0.0,
            "table column width must be finite and positive"
        );
        Self {
            title: title.into(),
            width,
        }
    }

    /// Returns the column title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the preferred column width.
    #[must_use]
    pub const fn width(&self) -> f32 {
        self.width
    }
}

struct VisibleTableRow {
    index: usize,
    key: u64,
    layouts: Vec<Arc<TextLayout>>,
    y: f32,
}

struct TableVisual {
    texts: Vec<String>,
    layouts: Vec<Arc<TextLayout>>,
}

impl<D: VirtualTableSource> VirtualTable<D> {
    /// Creates a table backed by `data` and explicit `columns`.
    #[must_use]
    pub fn new(data: D, columns: Vec<Column>) -> Self {
        Self {
            data,
            columns,
            row_height: ROW_HEIGHT,
            header_height: HEADER_HEIGHT,
            offset: 0.0,
            viewport: Size::ZERO,
            visible: Vec::new(),
            cache: HashMap::new(),
            selection: SelectionModel::new(),
            focused_column: 0,
            selection_changed: Signal::new(),
            activated: Signal::new(),
            keyboard_focus: false,
        }
    }

    /// Returns the backing data.
    #[must_use]
    pub const fn data(&self) -> &D {
        &self.data
    }

    /// Returns mutable access to the backing data and invalidates cell visuals.
    pub fn data_mut(&mut self) -> &mut D {
        self.cache.clear();
        &mut self.data
    }

    /// Returns the table's columns.
    #[must_use]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Returns the selection model.
    #[must_use]
    pub const fn selection(&self) -> &SelectionModel {
        &self.selection
    }

    /// Returns mutable access to the selection model.
    pub fn selection_mut(&mut self) -> &mut SelectionModel {
        &mut self.selection
    }

    /// Emits sorted selected row indices whenever selection changes.
    #[must_use]
    pub fn selection_changed(&self) -> Signal<Vec<usize>> {
        self.selection_changed.clone()
    }

    /// Emits a row and column when the user activates a cell.
    #[must_use]
    pub fn activated(&self) -> Signal<(usize, usize)> {
        self.activated.clone()
    }

    /// Returns the focused column index.
    #[must_use]
    pub const fn focused_column(&self) -> usize {
        self.focused_column
    }

    /// Returns the number of rows measured for the current viewport.
    #[must_use]
    pub fn visible_row_count(&self) -> usize {
        self.visible.len()
    }

    /// Returns the number of shaped rows retained for key-based reuse.
    #[must_use]
    pub fn cached_row_count(&self) -> usize {
        self.cache.len()
    }

    fn max_offset(&self) -> f32 {
        let content = row_extent(self.data.row_count(), self.row_height);
        (content - self.body_height()).max(0.0)
    }

    fn body_height(&self) -> f32 {
        (self.viewport.height() - self.header_height).max(0.0)
    }

    fn announce_selection(&self) {
        self.selection_changed.emit(
            &self
                .selection
                .selected()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
        );
    }

    fn row_at(&self, position: Point) -> Option<usize> {
        let y = position.y - self.header_height + self.offset;
        if y < 0.0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = (y / self.row_height) as usize;
        (row < self.data.row_count()).then_some(row)
    }

    fn ensure_visible(&mut self, row: usize) {
        let top = index_offset(row, self.row_height);
        let bottom = top + self.row_height;
        let height = self.body_height().max(self.row_height);
        if top < self.offset {
            self.offset = top;
        } else if bottom > self.offset + height {
            self.offset = bottom - height;
        }
        self.offset = self.offset.clamp(0.0, self.max_offset());
    }

    fn move_focus(&mut self, context: &mut EventContext<'_>, row: usize, extend: bool) {
        let count = self.data.row_count();
        if count == 0 {
            return;
        }
        let row = row.min(count - 1);
        let changed = self.selection.focus(row, extend);
        self.ensure_visible(row);
        if changed {
            self.announce_selection();
        }
        context.request_redraw();
        context.request_layout();
    }

    fn handle_key(
        &mut self,
        context: &mut EventContext<'_>,
        key: &drawen_core::KeyEvent,
    ) -> EventStatus {
        let extend = key.modifiers.contains(Modifiers::SHIFT);
        let command =
            key.modifiers.contains(Modifiers::CONTROL) || key.modifiers.contains(Modifiers::META);
        let count = self.data.row_count();
        let focused = self.selection.focused().unwrap_or(0);
        match &key.key {
            Key::Named(NamedKey::ArrowUp) => {
                self.move_focus(context, focused.saturating_sub(1), extend);
                EventStatus::Handled
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.move_focus(context, (focused + 1).min(count.saturating_sub(1)), extend);
                EventStatus::Handled
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.focused_column = self.focused_column.saturating_sub(1);
                context.request_redraw();
                EventStatus::Handled
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.focused_column =
                    (self.focused_column + 1).min(self.columns.len().saturating_sub(1));
                context.request_redraw();
                EventStatus::Handled
            }
            Key::Named(NamedKey::Home) => {
                self.move_focus(context, 0, extend);
                EventStatus::Handled
            }
            Key::Named(NamedKey::End) => {
                self.move_focus(context, count.saturating_sub(1), extend);
                EventStatus::Handled
            }
            Key::Named(NamedKey::Space) => {
                if count > 0 {
                    let changed = self.selection.toggle(focused.min(count - 1));
                    if changed {
                        self.announce_selection();
                        context.request_redraw();
                    }
                }
                EventStatus::Handled
            }
            Key::Named(NamedKey::Enter) => {
                if count > 0 {
                    self.activated
                        .emit(&(focused.min(count - 1), self.focused_column));
                }
                EventStatus::Handled
            }
            Key::Character(character) if command && character.eq_ignore_ascii_case("a") => {
                if self.selection.select_all(count) {
                    self.announce_selection();
                    context.request_redraw();
                }
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }
}

impl<D: VirtualTableSource> Widget for VirtualTable<D> {
    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("virtual-table")
    }

    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let environment = context.environment();
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            DEFAULT_WIDTH
        };
        let height = if max.height().is_finite() {
            max.height()
        } else {
            DEFAULT_HEIGHT
        };
        self.viewport = constraints.constrain(size(width, height));
        self.selection.retain(self.data.row_count());
        self.focused_column = self
            .focused_column
            .min(self.columns.len().saturating_sub(1));
        self.offset = self.offset.clamp(0.0, self.max_offset());

        let font = environment.font_for(&style);
        let text_style =
            TextStyle::new_with_font(font, style.foreground_or(environment.theme().foreground()));
        let first = self.first_visible();
        let last = self.last_visible();
        self.visible.clear();
        for row in first..last {
            let key = self.data.row_key(row);
            let texts: Vec<String> = (0..self.columns.len())
                .map(|column| self.data.cell_text(row, column))
                .collect();
            let layouts = {
                let cached = self
                    .cache
                    .get(&key)
                    .filter(|visual| visual.texts == texts)
                    .map(|visual| visual.layouts.clone());
                if let Some(layouts) = cached {
                    layouts
                } else {
                    let layouts: Vec<Arc<TextLayout>> = texts
                        .iter()
                        .map(|text| {
                            Arc::new(environment.text_shaper().layout(text, &text_style, None))
                        })
                        .collect();
                    self.cache.insert(
                        key,
                        TableVisual {
                            texts,
                            layouts: layouts.clone(),
                        },
                    );
                    layouts
                }
            };
            self.visible.push(VisibleTableRow {
                index: row,
                key,
                layouts,
                y: index_offset(row, self.row_height) - self.offset,
            });
        }
        if self.cache.len() > CACHE_LIMIT {
            let visible: BTreeSet<u64> = self.visible.iter().map(|row| row.key).collect();
            self.cache.retain(|key, _| visible.contains(key));
        }
        LayoutResult::new(self.viewport)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &DssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &DssStyle,
    ) {
        let theme = environment.theme();
        let foreground = style.foreground_or(theme.foreground());
        let accent = style.accent_or(theme.accent());
        let muted = theme.control_background();
        context.fill_rounded_rect(
            bounds,
            style.corner_radius_or(theme.corner_radius()),
            style.background_or(theme.background()),
        );
        let width = bounds.size.width();
        let column_widths = self.column_widths(width);
        let header = Rect::new(bounds.origin, size(width, self.header_height));
        context.fill_rect(header, muted);
        context.with_clip(header, |context| {
            let header_style = TextStyle::new_with_font(environment.font_for(style), foreground);
            let mut x = bounds.origin.x;
            for (index, column) in self.columns.iter().enumerate() {
                let cell = Rect::new(
                    Point::new(x, bounds.origin.y),
                    size(column_widths[index], self.header_height),
                );
                let layout = environment
                    .text_shaper()
                    .layout(&column.title, &header_style, None);
                context.with_clip(cell.inset(default_insets()), |context| {
                    context.draw_text(
                        layout,
                        Point::new(cell.origin.x + HORIZONTAL_PADDING, cell.origin.y + 7.0),
                    );
                });
                x += column_widths[index];
                context.fill_rect(
                    Rect::new(
                        Point::new(x - 1.0, bounds.origin.y),
                        size(1.0, self.header_height),
                    ),
                    theme.control_background(),
                );
            }
        });

        let body = Rect::new(
            Point::new(bounds.origin.x, bounds.origin.y + self.header_height),
            size(width, self.body_height()),
        );
        context.with_clip(body, |context| {
            for row in &self.visible {
                let row_bounds = Rect::new(
                    Point::new(body.origin.x, body.origin.y + row.y),
                    size(width, self.row_height),
                );
                if self.selection.contains(row.index) {
                    context.fill_rect(row_bounds, accent.with_alpha(0.22));
                } else if self.selection.focused() == Some(row.index) && self.keyboard_focus {
                    context.stroke_rect(row_bounds, 1.0, accent);
                }
                let mut x = row_bounds.origin.x;
                for (column, layout) in row.layouts.iter().enumerate() {
                    let cell = Rect::new(
                        Point::new(x, row_bounds.origin.y),
                        size(column_widths[column], self.row_height),
                    );
                    context.with_clip(cell.inset(default_insets()), |context| {
                        context.draw_text(
                            (**layout).clone().with_color(foreground),
                            Point::new(
                                cell.origin.x + HORIZONTAL_PADDING,
                                cell.origin.y + (self.row_height - layout.size().height()) / 2.0,
                            ),
                        );
                    });
                    x += column_widths[column];
                }
                if self.keyboard_focus && self.selection.focused() == Some(row.index) {
                    let x = bounds.origin.x
                        + column_widths[..self.focused_column.min(column_widths.len())]
                            .iter()
                            .sum::<f32>();
                    let cell = Rect::new(
                        Point::new(x, row_bounds.origin.y),
                        size(
                            column_widths
                                .get(self.focused_column)
                                .copied()
                                .unwrap_or(0.0),
                            self.row_height,
                        ),
                    );
                    context.stroke_rect(cell, 2.0, accent);
                }
            }
        });
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) => {
                if pointer.button != Some(PointerButton::Primary) {
                    return EventStatus::Ignored;
                }
                let Some(row) = self.row_at(pointer.position) else {
                    return EventStatus::Ignored;
                };
                context.request_focus();
                context.capture_pointer(pointer.pointer_id);
                if let Some(column) = self.column_at(pointer.position) {
                    self.focused_column = column;
                }
                let changed = if pointer.modifiers.contains(Modifiers::CONTROL) {
                    self.selection.toggle(row)
                } else {
                    self.selection
                        .focus(row, pointer.modifiers.contains(Modifiers::SHIFT))
                };
                if changed {
                    self.announce_selection();
                }
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) => {
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::Wheel(wheel) => {
                let delta = match wheel.delta {
                    WheelDelta::Pixels(delta) => delta.y,
                    WheelDelta::Lines(delta) => delta.y * self.row_height * 3.0,
                };
                let old = self.offset;
                self.offset = (self.offset - delta).clamp(0.0, self.max_offset());
                if (self.offset - old).abs() <= f32::EPSILON {
                    return EventStatus::Ignored;
                }
                context.request_layout();
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) => self.handle_key(context, key),
            InputEvent::FocusChanged(FocusChanged { focused }) => {
                self.keyboard_focus = *focused == Some(context.current_target());
                context.request_redraw();
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let count = self.data.row_count();
        let selected = self.selection.selected().len();
        let mut accessibility = Accessibility::new(AccessibilityRole::Table)
            .with_value(format!("{selected} of {count} selected"))
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(AccessibilityActions::FOCUS | AccessibilityActions::SELECT);
        if self.selection.mode() == SelectionMode::Multiple {
            accessibility = accessibility.with_states(AccessibilityState::MULTISELECTABLE);
        }
        if let Some(label) = self.data.label() {
            accessibility = accessibility.with_label(label);
        }
        accessibility
    }

    fn accessibility_action(&mut self, action: &AccessibilityAction) -> bool {
        match action {
            AccessibilityAction::Select => {
                let row = self
                    .selection
                    .focused()
                    .or_else(|| (self.data.row_count() > 0).then_some(0));
                row.is_some_and(|row| self.selection.select_only(row))
            }
            _ => false,
        }
    }
}

impl<D: VirtualTableSource> VirtualTable<D> {
    fn first_visible(&self) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = (self.offset / self.row_height) as usize;
        row.saturating_sub(OVERSCAN)
    }

    fn last_visible(&self) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = ((self.offset + self.body_height()) / self.row_height).ceil() as usize;
        (row + OVERSCAN + 1).min(self.data.row_count())
    }

    fn column_widths(&self, available: f32) -> Vec<f32> {
        let preferred: f32 = self.columns.iter().map(Column::width).sum();
        if preferred <= 0.0 {
            return vec![0.0; self.columns.len()];
        }
        let scale = if available > 0.0 {
            available / preferred
        } else {
            1.0
        };
        self.columns
            .iter()
            .map(|column| column.width * scale)
            .collect()
    }

    fn column_at(&self, position: Point) -> Option<usize> {
        let mut x = 0.0;
        let widths = self.column_widths(self.viewport.width());
        for (index, width) in widths.into_iter().enumerate() {
            if position.x < x + width {
                return Some(index);
            }
            x += width;
        }
        None
    }
}

#[allow(clippy::cast_precision_loss)]
fn row_extent(count: usize, row_height: f32) -> f32 {
    count as f32 * row_height
}

#[allow(clippy::cast_precision_loss)]
fn index_offset(index: usize, row_height: f32) -> f32 {
    index as f32 * row_height
}

fn default_insets() -> drawen_core::Insets {
    drawen_core::Insets::new(0.0, HORIZONTAL_PADDING, 0.0, HORIZONTAL_PADDING)
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("virtual view sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use drawen_core::{
        Constraints, InputEvent, Key, KeyCode, Modifiers, NamedKey, Point, PointerButton,
        PointerButtons, PointerEvent, PointerId, Size,
    };
    use drawen_render::{DisplayCommand, DisplayList, PaintContext};
    use drawen_ui::UiRuntime;

    use super::*;

    struct Rows(usize);

    impl VirtualDataSource for Rows {
        fn row_count(&self) -> usize {
            self.0
        }

        fn row_key(&self, row: usize) -> u64 {
            u64::try_from(row).expect("test rows fit u64")
        }

        fn row_text(&self, row: usize) -> String {
            format!("Row {row}")
        }
    }

    impl VirtualTableSource for Rows {
        fn row_count(&self) -> usize {
            self.0
        }

        fn row_key(&self, row: usize) -> u64 {
            u64::try_from(row).expect("test rows fit u64")
        }

        fn column_count(&self) -> usize {
            3
        }

        fn column_title(&self, column: usize) -> String {
            format!("Column {column}")
        }

        fn column_width(&self, _: usize) -> f32 {
            60.0
        }

        fn cell_text(&self, row: usize, column: usize) -> String {
            format!("{row}:{column}")
        }
    }

    fn viewport(width: f32, height: f32) -> Constraints {
        Constraints::tight(Size::new(width, height).expect("valid viewport"))
            .expect("valid tight constraints")
    }

    fn pointer_down(x: f32, y: f32) -> InputEvent {
        InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(x, y),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        })
    }

    fn key(named: NamedKey) -> InputEvent {
        InputEvent::KeyDown(drawen_core::KeyEvent {
            key: Key::Named(named),
            code: KeyCode::Unidentified,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
    }

    fn draw_text_count(list: &DisplayList) -> usize {
        list.commands()
            .iter()
            .filter(|command| matches!(command, DisplayCommand::DrawText { .. }))
            .count()
    }

    #[test]
    fn virtual_list_shapes_only_the_visible_window_and_reuses_keys() {
        let mut runtime = UiRuntime::new(VirtualList::new(Rows(10_000), 20.0));
        runtime
            .layout(viewport(200.0, 100.0))
            .expect("layout succeeds");
        let mut first = DisplayList::new();
        runtime
            .paint(&mut PaintContext::new(&mut first))
            .expect("paint succeeds");
        let visible = draw_text_count(&first);
        assert!(
            (1..=8).contains(&visible),
            "unexpected visible rows: {visible}"
        );

        let scroll = InputEvent::Wheel(drawen_core::WheelEvent {
            position: Point::new(10.0, 10.0),
            delta: drawen_core::WheelDelta::Pixels(Point::new(0.0, -60.0)),
            modifiers: Modifiers::NONE,
        });
        assert!(runtime.dispatch_event(&scroll).is_handled());
        runtime
            .layout(viewport(200.0, 100.0))
            .expect("layout succeeds");
        let mut second = DisplayList::new();
        runtime
            .paint(&mut PaintContext::new(&mut second))
            .expect("paint succeeds");
        let scrolled = draw_text_count(&second);
        assert!(
            (1..=8).contains(&scrolled),
            "unexpected visible rows after scrolling: {scrolled}"
        );
    }

    #[test]
    fn keyboard_navigation_moves_and_extends_selection() {
        let list = VirtualList::new(Rows(100), 20.0);
        let selection = list.selection_changed();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let subscription = selection.subscribe({
            let observed = Rc::clone(&observed);
            move |rows| observed.borrow_mut().push(rows.clone())
        });
        let mut runtime = UiRuntime::new(list);
        runtime
            .layout(viewport(200.0, 100.0))
            .expect("layout succeeds");
        let _ = runtime.dispatch_event(&pointer_down(10.0, 10.0));
        runtime
            .layout(viewport(200.0, 100.0))
            .expect("layout succeeds");
        assert!(
            runtime
                .dispatch_event(&key(NamedKey::ArrowDown))
                .is_handled()
        );
        runtime
            .layout(viewport(200.0, 100.0))
            .expect("layout succeeds");

        assert_eq!(*observed.borrow(), vec![vec![0], vec![1]]);
        drop(subscription);
        drop(selection);
    }

    #[test]
    fn virtual_table_keeps_headers_and_rows_virtualized() {
        let mut runtime = UiRuntime::new(VirtualTable::new(
            Rows(10_000),
            vec![
                Column::new("A", 60.0),
                Column::new("B", 60.0),
                Column::new("C", 60.0),
            ],
        ));
        runtime
            .layout(viewport(180.0, 120.0))
            .expect("layout succeeds");
        let mut list = DisplayList::new();
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("paint succeeds");
        let draws = draw_text_count(&list);
        assert!((3..=24).contains(&draws), "unexpected table draws: {draws}");

        let _ = runtime.dispatch_event(&pointer_down(80.0, 40.0));
        let semantics = runtime.semantic_tree();
        let table = semantics
            .nodes()
            .iter()
            .find(|node| node.role() == AccessibilityRole::Table)
            .expect("table is exposed");
        assert_eq!(table.value(), Some("1 of 10000 selected"));
    }
}
