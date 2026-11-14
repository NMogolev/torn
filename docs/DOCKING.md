# Docking, MDI and persisted workspace

## Goal

Drawen supports the practical desktop workflow associated with wxAUI: panels can
be docked to an edge, tabbed, split, floated, hidden, and restored between
launches. Documents also have an MDI-like Qt presentation: a central tabbed
document area by default, with an optional bounded free-form child-window mode
for applications that need it.

The workspace is an application-level layer on top of the retained widget tree.
It relies on the native window adapter, focus and pointer capture, clipping,
keyboard routing, and drag interaction.

## Public model

The workspace owns a **layout model**, not application panels. An application
registers a stable `PanelId` (normally an application-defined string) with a
factory that can construct the current panel widget. The model stores only that
ID and its placement. It never contains `Widget`, callbacks, or platform
handles.

The layout tree has a deliberately small set of node kinds:

- `Split { axis, ratio, first, second }` — horizontal or vertical split; the
  ratio is normalized and clamped to a sensible range during restore.
- `Tabs { items, active }` — ordered dock tabs and selected tab.
- `Panel { id }` — a registered tool panel.
- `Documents { presentation, items, active }` — the central document region.
- `Empty` — an explicit empty location, valid as the root; an empty split
  branch collapses to its surviving sibling during normalization.

Floating panels and free-form MDI documents are separate records containing a
stable ID, bounds in logical pixels, visibility, and z-order. The root tree
describes docked content only. This prevents edge cases where a floating item
is accidentally owned by two layout branches. Z-orders are normalized to dense
consecutive ranks after every mutation, so raising a window never grows the
counter without bound and the stacking order stays unambiguous.

## Runtime behavior

`DockArea` projects the model onto retained child widgets. The application keeps
a shared `WorkspaceLayout`, registers stable panel/document IDs with the area,
and appends the corresponding child widgets in the same order. A layout pass
places docked panels, selected tabs, and selected documents; inactive, hidden,
and floating items remain retained but are not painted or hit-tested in the
docked area. Unregistered IDs are rendered as placeholders.

Interactive changes are expressed as model operations: `split_panel`,
`move_to_tabs`, `float_panel`, `dock_panel`, `hide_panel`, `show_panel`,
`activate_panel`, `remove_panel`, `float_document`, `dock_document`,
`activate_document`, and `close_document`. Floating records have their own
transactional `move_floating_panel`, `resize_floating_panel`,
`bring_panel_to_front` operations and the corresponding document variants.
Every operation validates the tree, normalizes it, and preserves each ID at most
once. The application requests the next layout and chooses when to persist the
updated model.

Normalization is an internal step of every successful transaction. An empty
`Split` branch is replaced by its surviving branch, and a `Tabs` group with one
item becomes a `Panel`. Ordinary operations never leave empty tab groups behind:
an empty group becomes `Empty`, which may in turn disappear during split
collapsing. `Documents` remains in place even when its item list becomes empty,
because it identifies the central document region. This keeps drag-and-drop
sequences from accumulating technical containers.

`move_floating_*` and `resize_floating_*` change bounds transactionally without
exposing the underlying records. `bring_*_to_front` raises exactly one floating
item; the transaction then renumbers both floating collections to consecutive
ranks. `remove_panel` frees a panel from docked, floating, and hidden storage,
while `close_document` frees a document from both the central area and free-form
storage. When the active tab disappears, the next tab is selected, or the
previous tab when the removed item was last.

Dragging is a transaction: compute a preview target without mutating the
model, then apply exactly one operation on pointer release. Escape cancels the
transaction. This avoids intermediate invalid layouts and reduces unnecessary
persistence writes.

## Persistence contract

The on-disk representation is a versioned, human-readable JSON document.
Serialization lives in `drawen-workspace` and uses `serde` behind the default
`serde` feature; the core widget runtime stays dependency-free.

```text
WorkspaceLayout {
    version,
    root,
    floating,
    hidden,
    freeform_documents,
}
```

Restoring follows these rules:

1. Parse and validate before replacing the active layout.
2. If parsing or validation fails, retain the current/default layout and report
   a diagnostic; never crash during startup.
3. Clamp ratios and window bounds to the current monitor work area after the
   platform adapter supplies it. `clamp_floating_bounds` performs the
   model-side step while preserving at least `MIN_VISIBLE_FLOATING_EXTENT`
   logical pixels of each window. The platform layer owns monitor discovery,
   DPI conversion, and disconnected-monitor handling; the workspace model never
   queries monitors itself.
4. Preserve unknown IDs as visible “missing panel” placeholders. A later
   version of the application can replace the placeholder when that ID is
   registered again.
5. Migrate older format versions explicitly through the internal migration
   hook shared by restoration and JSON decoding. Newer, unsupported versions
   are not overwritten automatically.

The application owns file I/O and its configuration path. Drawen provides the
versioned value type plus serialization helpers, which keeps persistence policy
(autosave timing, profiles, backup files, and recovery) under application
control.

## Styling

Docking chrome consumes semantic values from `Theme`: surface/background,
foreground, accent, spacing, font size, and corner radius. It does not depend on
a stylesheet parser. The `.dss` layer resolves a cascade into the same typed
values, so adopting CSS-like styles does not alter layout or persistence data.

Custom window chrome has its own semantic values: `title_bar_background`,
`title_bar_foreground`, `title_bar_hover_background`, `muted_foreground`, and
`danger`. See [WINDOW_CHROME.md](WINDOW_CHROME.md) for the title-bar contract.
