# Torn Style Sheets (TSS)

TSS is Torn's CSS-inspired stylesheet format. It resolves visual declarations
into Torn's typed style values; it does not expose stringly typed CSS concepts
to widgets or renderers.

The runtime applies TSS per retained node. Every node resolves a `StyleTarget`
built from its widget type, id, classes, and interaction state, then receives a
typed `TssStyle` before layout and again before paint. Widgets consume that
style directly, and inheritable text properties flow to descendants.

## Using a stylesheet

Pass source directly to `UiEnvironment::from_tss`, with an existing semantic
theme as a fallback:

```rust
use torn::{DarkTheme, UiEnvironment};

let environment = UiEnvironment::from_tss(
    DarkTheme,
    r#"
        :root {
            background: #101820;
            color: #f1f3f5;
            accent-color: #2edcb3;
            gap: 12px;
            font-size: 15px;
            border-radius: 6px;
        }

        button {
            background: #243447;
            padding: 6px 10px;
        }

        button:hover { background: #34516b; }
        button:pressed { background: #182838; }
        button:focus { border: 2px solid #2edcb3; }

        #save { background: #1f7a5a; }
        .primary { border-radius: 8px; }
        .danger { color: #ff6b6b; }
    "#,
)?;
```

The semantic theme remains the fallback for every value the stylesheet does not
supply. `:root` declarations become global theme tokens and the inheritance
root; type, id, class, and state rules stay as per-node styles. Adding TSS does
not require copying a complete palette, and a `button` rule no longer rewrites
the theme value used by unrelated widgets.

## Supported selectors and cascade

TSS accepts one compound selector per rule, or a comma-separated list of
compound selectors:

```tss
button { background: #243447; }
.primary { background: #336699; }
#save.primary:hover { background: rgba(12, 34, 56, 0.5); }
button:hover, .preview { border: 2px solid #abcdef; }
```

Supported selector parts are:

| Part | Meaning |
| --- | --- |
| `*` | Any non-root target. |
| `button` | A widget type name. |
| `#save` | A target id. |
| `.primary` | A target class. |
| `:hover`, `:pressed`, `:focus`, `:disabled` | Interaction state. |
| `:root` | The stylesheet root and global theme values. |

Matching declarations are merged by CSS specificity (id, then class/state, then
type) and source order. Rules are sorted once during parsing, so resolving a
target does not allocate or sort a per-node rule list.

## Per-node styling

Widgets identify themselves through `Widget::style_target()`:

```rust
use torn::{PseudoClass, StyleTarget};

fn style_target(&self) -> StyleTarget {
    let target = StyleTarget::new("button");
    if self.pressed {
        target.with_state(PseudoClass::Pressed)
    } else {
        target
    }
}
```

The runtime augments that target with `:hover` and `:focus` from retained
input state. Standard widgets publish their `:pressed` or selection state where
relevant. During the style pass the runtime resolves each node's target,
inherits `color`, `font-size`, and `accent-color` from its parent, and stores
the result on the node. `LayoutContext::style()` exposes it while laying out;
`paint_styled` receives it while painting.

Use `Styled<W>` to add selectors without changing a widget's API:

```rust
use torn::{Button, Styled};

let save = Styled::new(Button::new())
    .with_id("save")
    .with_class("primary")
    .with_class("danger");
```

`Styled` forwards layout, painting, events, and focus behavior while preserving
the wrapped widget's type name, so the button above matches `button`,
`#save`, `.primary`, and `.danger`.

Custom widgets can resolve a style through the same contract with
`UiEnvironment::style_for(&target)`, or read `LayoutContext::style()` for the
node currently being laid out. There is no separate path that silently drops a
declaration.

Standard widget type names include `box`, `text`, `button`, `checkbox`,
`radio`, `slider`, `text-field`, `progress-bar`, `menu-bar`, `toolbar`,
`tab-bar`, `dropdown`, `splitter`, `dock-area`, `list-view`, `scroll-view`,
`padding`, `align`, `sized-box`, `image`, and `stack`.

## Supported properties

| Property | Value | Applied to |
| --- | --- | --- |
| `background`, `background-color` | `transparent`, `black`, `white`, `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb(...)`, or `rgba(...)`. | Surfaces such as boxes, buttons, fields, track backgrounds, bars, and docks. |
| `color` | A supported color. | Text and glyphs; inherits to descendants. |
| `accent-color` | A supported color. | Focus, selection, caret, divider, and filled-control values; inherits to descendants. |
| `border` | `<length> solid <color>`. | Surface outlines. |
| `padding` | One to four lengths. | Inner spacing of boxes, buttons, fields, toolbars, and similar containers. |
| `gap` | A non-negative length. | Spacing between labels and controls or toolbar items. |
| `font-size` | A non-negative length. | Text shaped during layout; inherits to descendants. |
| `border-radius` | A non-negative length. | Rounded surfaces. |

Non-zero lengths require the `px` unit. TSS accepts `0` without a unit. The
alpha component of `rgba` is a number from `0` through `1` or a percentage.
Comments use the familiar `/* comment */` form.

Unknown properties and unsupported syntax produce `TssError`; Torn never
silently drops a style declaration. Selectors must be a valid compound form:
`*button` is rejected rather than silently treated as `button`, while `*.primary`
remains valid.

## Deliberately deferred

The current parser has no descendant or sibling combinators, variables,
expressions, percentages, media queries, keyframes, animations, or full
browser-CSS compatibility. Inheritance currently covers `color`, `font-size`,
and `accent-color`; surface properties do not inherit. These features will only
be added when Torn has the corresponding runtime and layout contracts.
