# Torn Style Sheets (TSS)

TSS is Torn's CSS-inspired stylesheet format. It resolves visual declarations
into Torn's typed style values; it does not expose stringly typed CSS concepts
to widgets or renderers.

The first implementation intentionally covers the visual values already owned
by `Theme`. This permits applications to adopt a stylesheet immediately while
the retained tree gains per-node style metadata in a later iteration.

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
    "#,
)?;
```

The environment uses the result as its `Theme`. Any semantic value absent from
the stylesheet continues to come from `DarkTheme`, so adding TSS does not
require copying a complete palette.

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
| `button` | Application-defined type name. |
| `#save` | A target id. |
| `.primary` | A target class. |
| `:hover`, `:pressed`, `:focus`, `:disabled` | Interaction state. |
| `:root` | The stylesheet root and global theme values. |

Matching rules are merged by CSS specificity (`id`, then
class/state, then type) and source order. The public `StyleSheet::resolve`
method can resolve a `StyleTarget` for custom widgets and tooling today.

The runtime does not yet attach ids and classes to retained nodes. Consequently,
`UiEnvironment::from_tss` currently applies `:root`, `button`, `button:hover`,
and `button:pressed` through `Theme`; arbitrary id/class rules are available
through `StyleSheet::resolve` until node metadata is introduced.

## Supported properties

| Property | Value |
| --- | --- |
| `background`, `background-color` | `transparent`, `black`, `white`, `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb(...)`, or `rgba(...)`. |
| `color` | A supported color. |
| `accent-color` | A supported color. |
| `border` | `<length> solid <color>`. |
| `padding` | One to four lengths. |
| `gap` | A non-negative length. |
| `font-size` | A non-negative length. |
| `border-radius` | A non-negative length. |

Non-zero lengths require the `px` unit. TSS accepts `0` without a unit. The
alpha component of `rgba` is a number from `0` through `1` or a percentage.
Comments use the familiar `/* comment */` form.

Unknown properties and unsupported syntax produce `TssError`; Torn never
silently drops a style declaration.

## Deliberately deferred

The current parser has no descendant or sibling combinators, inheritance,
variables, expressions, percentages, media queries, keyframes, animations, or
full browser-CSS compatibility. These features will only be added when Torn has
the corresponding runtime and layout contracts.
