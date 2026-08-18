# Docking, MDI and persisted workspace

## Goal

Torn should support the practical desktop workflow associated with wxAUI:
panels can be docked to an edge, tabbed, split, floated, hidden, and restored
between launches. Documents should also have an MDI-like Qt presentation: a
central tabbed document area by default, with an optional bounded free-form
child-window mode for applications that need it.

This is a later desktop-workspace feature, not a replacement for the normal
retained widget tree. It depends on a native window adapter, complete focus and
pointer capture, clipping, keyboard routing, and drag interaction.

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
- `Empty` — an intentional empty slot retained while the user rearranges the
  workspace.

Floating panels and free-form MDI documents are separate records containing a
stable ID, bounds in logical pixels, visibility, and z-order. The root tree
describes docked content only. This prevents edge cases where a floating item
is accidentally owned by two layout branches.

## Runtime behavior

`DockArea` projects the model onto retained child widgets. The application keeps
a shared `WorkspaceLayout`, registers stable panel/document IDs with the area,
and appends the corresponding child widgets in the same order. A layout pass
places docked panels, selected tabs, and selected documents; inactive, hidden,
and floating items remain retained but are not painted or hit-tested in the
docked area. Unregistered IDs are rendered as placeholders.

Interactive changes are expressed as model operations, such as `split`,
`move_to_tabs`, `float`, `dock`, `hide`, `show`, and `activate`. Every operation
validates the tree and preserves each ID at most once. The application requests
the next layout and chooses when to persist the updated model.

Dragging is a transaction: compute a preview target without mutating the
model, then apply exactly one operation on pointer release. Escape cancels the
transaction. This avoids intermediate invalid layouts and reduces unnecessary
persistence writes.

## Persistence contract

The initial on-disk representation should be a versioned, human-readable JSON
document. Serialization support belongs in the workspace/style layer and can
use an optional `serde` feature; the core widget runtime stays dependency-free.

```text
WorkspaceLayout {
    version,
    root,
    floating,
    hidden,
    document_presentation,
}
```

Restoring follows these rules:

1. Parse and validate before replacing the active layout.
2. If parsing or validation fails, retain the current/default layout and report
   a diagnostic; never crash during startup.
3. Clamp ratios and window bounds to the current monitor work area after the
   platform adapter supplies it.
4. Preserve unknown IDs as visible “missing panel” placeholders. A later
   version of the application can replace the placeholder when that ID is
   registered again.
5. Migrate older format versions explicitly. Newer, unsupported versions are
   not overwritten automatically.

The application owns file I/O and its configuration path. Torn provides the
versioned value type plus serialization helpers, which keeps persistence policy
(autosave timing, profiles, backup files, and recovery) under application
control.

## Styling

Docking chrome consumes semantic values from `Theme`: surface/background,
foreground, accent, spacing, font size, and corner radius. It must not depend
on a stylesheet parser. The future `.tss` layer resolves a cascade into the
same typed values, so adopting CSS-like styles later does not alter layout or
persistence data.

The initial theme contract is intentionally compact. Dock-tab, splitter, and
title-bar-specific colors will be introduced only once their state model and
contrast requirements are known.
