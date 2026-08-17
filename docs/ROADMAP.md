# TORN v0 Roadmap

TORN (*Toolkit for Object-oriented Rendering and Native widgets*) is a Rust UI
toolkit built around a retained widget tree, backend-independent rendering, and
composition. “Object-oriented” describes the toolkit architecture; public
widgets use Rust traits and composition rather than inheritance.

## Product principles

- **Rust-native API.** No required base-object inheritance, unchecked
  downcasts, or global dynamically typed event system.
- **Retained widget tree.** The framework owns stable widget nodes and their
  layout, input-routing, focus, and invalidation metadata.
- **Backend-independent paint.** Widgets record drawing commands; renderer
  backends execute them.
- **Box constraints before solver constraints.** Every layout receives
  `min`/`max` size constraints, measures itself, then lays out children.
  A Cassowary/Auto Layout-style solver is explicitly out of scope for v0.
- **Composition first.** Composite controls expose content slots rather than
  assuming fixed child types. For example, a button may contain any one widget.
- **Correctness before optimization.** The first renderer repaints every
  frame. Dirty state is tracked from the start but partial repainting follows
  only after the basic pipeline is tested and profiled.
- **Testable internals.** Geometry, layout, display-list generation, and
  software rendering must be deterministic and usable without a window.

## Architectural decisions

### Widget model

The UI runtime owns nodes in a generational arena, keyed by `WidgetId`. A node
contains framework-managed data such as parent/child relationships, bounds,
focus eligibility, and dirty flags. A user-defined widget owns only its state
and behavior.

This avoids a mandatory public `WidgetCore` field and gives the runtime a safe
place to retain tree metadata without inheriting from a base class.

The lifecycle is:

```text
update state -> invalidate -> measure -> layout -> paint -> display list -> renderer
```

The core widget contract will evolve around these operations:

```rust
trait Widget {
    fn measure(&mut self, ctx: &mut LayoutContext<'_>, constraints: Constraints) -> Size;
    fn layout(&mut self, ctx: &mut LayoutContext<'_>, bounds: Rect);
    fn paint(&self, ctx: &mut PaintContext<'_>);
    fn handle_event(&mut self, ctx: &mut EventContext<'_>, event: &InputEvent) -> EventStatus;
}
```

Exact visibility and method defaults will be settled during the tree-runtime
milestone, not frozen prematurely.

### Geometry and layout

The foundational types are `Point`, `Size`, `Rect`, `Insets`, `Color`, and
`Constraints`.

`Constraints` preserves these invariants:

```text
0 <= min.width  <= max.width
0 <= min.height <= max.height
```

Initial layout primitives are `SizedBox`, `Padding`, `Row`, `Column`, `Stack`,
and `Align`. Widgets first report a size with `measure`; parents subsequently
assign concrete `Rect` bounds with `layout`.

### Input and signals

Platform input is framework-defined and is not a user-extensible enum:

```text
PointerDown, PointerMove, PointerUp, Wheel,
KeyDown, KeyUp, TextInput, FocusChanged
```

Dispatch follows hit-testing plus capture, target, and bubble phases:

```text
window -> ancestors -> target -> ancestors -> window
       capture       target      bubble
```

`EventContext` owns side effects such as stopping propagation, pointer capture,
focus changes, and invalidation. This allows a handler to request several
actions without an unscalable `EventResult` enum.

Widget notifications use typed, UI-thread-local `Signal<T>` values. They begin
without `Send`/`Sync` bounds. Connections return drop-based subscriptions and
emission must remain safe when a callback disconnects a subscription.

### Rendering and text

Widgets paint to `PaintContext`, which records a backend-neutral `DisplayList`.
Backends then execute it:

```text
Widget tree -> PaintContext -> DisplayList -> software / wgpu renderer
```

The first drawing operations are filled rectangles, clipping, and pre-laid-out
text. Text measurement/shaping is separated from drawing:

```rust
trait TextShaper {
    fn layout(&self, text: &str, style: &TextStyle, width: Option<f32>) -> TextLayout;
}
```

Paint commands consume `TextLayout` rather than raw strings, so layout metrics
and rendering cannot disagree.

### Styling and invalidation

v0 starts with the small typed [`Theme`](../crates/torn-ui/src/theme.rs)
contract: semantic background, foreground, and accent colors plus the default
spacing, font size, and corner radius. `DarkTheme` and `LightTheme` are built
in. `SystemTheme` is updated by the future platform adapter from the OS
appearance; the dependency-free UI crate must not query the OS itself.

A future `.tss` parser is deliberately deferred. It will resolve selectors and
cascade into these same typed visual values rather than exposing CSS concepts to
widgets. More state-specific values are added to `Theme` only when a widget
needs them and the semantic distinction is proven.

Dirty state is represented separately for `LAYOUT`, `PAINT`, and `STYLE`.
Changing text or padding invalidates layout and paint; a hover color normally
invalidates paint only. The initial runtime still creates and renders a full
frame each redraw.

## Workspace plan

The first workspace uses the smallest set of crates that avoids dependency
cycles and leaves rendering/platform backends optional:

```text
torn-core             Geometry, IDs, colors, input-data types
torn-render           Display list, paint API, text contracts
torn-ui               Widget runtime, layout lifecycle, routing, invalidation
torn-widgets          Standard layout and control widgets
torn-software         Deterministic reference renderer and test support
torn                  Public facade crate
examples/hello-torn   First interactive vertical-slice demonstration
```

Later, when the contracts are stable:

```text
torn-platform         Platform abstraction
torn-platform-winit   Winit window/event-loop adapter
torn-wgpu             GPU renderer
torn-style            Extracted theme/cascade engine, if warranted
```

This order intentionally avoids adding `winit`, `wgpu`, or a CSS parser before
the headless core is independently testable.

## Implementation milestones

### M0 — Foundation

Create the Cargo workspace and `torn-core`. Implement and test geometry,
constraints, IDs, colors, and input-data types. Establish formatting, Clippy,
and test commands.

**Done when:** a headless crate compiles; geometry and constraint invariants
have unit tests; no renderer or platform dependency exists.

### M1 — Display-list contract and software renderer

Create `torn-render` and `torn-software`. Add a minimal display list for filled
rectangles, clips, and text placeholders or an initial text contract. Render
small deterministic scenes into an in-memory pixel buffer.

**Done when:** test code can record a scene, render it without a window, and
assert pixels or compare a stable image fixture.

### M2 — Retained UI tree and layout

Create `torn-ui`. Implement generational widget IDs, tree ownership,
measure/layout lifecycle, dirty flags, hit testing, and initial layout
primitives (`SizedBox`, `Padding`, `Row`, `Column`, and `Stack`).

**Done when:** headless tests verify nested layout bounds, constraint behavior,
and hit-test results.

### M3 — Input, focus, and typed signals

Implement capture/target/bubble dispatch, propagation controls, pointer
capture, basic focus management, `Signal<T>`, and `Subscription`. Add test
widgets that record event order.

**Done when:** tests cover event phase order, stop propagation, pointer capture
outside the original bounds, and self-disconnecting signal handlers.

### M4 — First widgets and end-to-end scene

Create `torn-widgets` with `Container`, `Text`, `Button`, and basic styled
layout composition. A button paints different normal/hover/pressed states and
emits a typed click signal.

**Done when:** a headless test builds a panel containing text and a button,
lays it out, dispatches a click, and validates its display list or software
image.

### M5 — Windowed example

Add the platform abstraction, a `winit` adapter, and `hello-torn`. Connect a
native window to the existing runtime and a rendering backend. Keep this
example intentionally small: text plus one interactive button.

**Done when:** it opens a native window, handles pointer interaction, and
redraws correctly on Windows.

### M7 — Desktop workspace: docking and MDI

Add a desktop workspace after the window, focus, pointer-capture, and basic
widgets are stable. The workspace contains dockable panels and optional
document windows with an MDI presentation inspired by Qt: documents can be
tabbed in the central area or displayed as bounded, movable child windows.

The persistent model is a serializable data tree, independent from live
widgets. It records panel/document identifiers, split orientation and ratio,
tab order and active tab, floating-window geometry, and the currently selected
MDI presentation. Applications own stable IDs and decide how to recreate a
panel from one. Unknown, removed, or unavailable panels must be preserved as
placeholders rather than discarded, so saved layouts survive application
upgrades. Persist only this model and a format version; never serialize widget
objects, callbacks, or platform handles.

See [`DOCKING.md`](DOCKING.md) for the proposed contract and persistence rules.

### M6 — Capability expansion

Add scrolling, editable text, real text shaping and font fallback, themes,
accessibility design, GPU rendering, paint caching, and selective repainting,
in that order of demonstrated need.

## Explicit non-goals before M5

- Full CSS/TSS parsing, selectors, and cascade.
- General linear constraint solving.
- Multiple renderer and windowing backends.
- Cross-thread widget access.
- Accessibility implementation, while keeping its future requirements in mind.
- Native platform controls beyond the window and system integration.
- GPU batching, layers, filters, and dirty-region command skipping.

## Validation policy

Every milestone must pass, where applicable:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The project should favor unit tests for mathematical and routing contracts,
then headless software-renderer integration tests. Native-window smoke tests
are added only once the platform layer exists.
