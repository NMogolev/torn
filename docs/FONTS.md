# Fonts

Drawen models fonts the way Qt does: application code describes a **font
request**, and a `FontDatabase` resolves that request to concrete **font faces**
at shaping time. Measurement and painting always use the same resolved face, so
layout and rendering cannot disagree about which font was selected.

The implementation lives in [`drawen-render`](../crates/drawen-render/src/font.rs);
the environment-level defaults live in
[`drawen-ui`](../crates/drawen-ui/src/environment.rs).

## Requests and faces

A [`Font`](../crates/drawen-render/src/font.rs) is a request, analogous to
`QFont`:

| Component | Type | Meaning |
| --- | --- | --- |
| Families | `Vec<FontFamily>` | Preferred families, highest priority first. |
| Size | `FontSize` | `Pixels` or `Points` (resolved with a logical DPI). |
| Weight | `FontWeight` | CSS-compatible 1–1000; common values are constants. |
| Style | `FontStyle` | `Normal`, `Italic`, or `Oblique`. |

```rust
use drawen::{Font, FontFamily, FontSize, FontStyle, FontWeight};

let font = Font::new()
    .with_families([
        FontFamily::new("Inter"),
        FontFamily::new("Noto Sans"),
        FontFamily::new("sans-serif"),
    ])
    .with_size(FontSize::points(12.0))
    .with_weight(FontWeight::SEMI_BOLD)
    .with_style(FontStyle::Normal);
```

A request never guarantees that a family exists. It describes intent; the
database decides what is actually available.

A [`FontFace`](../crates/drawen-render/src/font.rs) is a concrete parsed font
resource with a `FontDescriptor` (family, weight, style). Faces expose:

- `family()`, `weight()`, `style()`, and `descriptor()`;
- `has_glyph(char)` for per-character coverage;
- `rasterize(glyph_id, size)` for an 8-bit coverage bitmap;
- `metrics(size)` for ascent, descent, leading, and line height.

## The font database

[`FontDatabase`](../crates/drawen-render/src/font.rs) owns registered faces and
performs Qt-style matching:

1. For each requested family, in order, choose the face in that family with the
   closest style and then the closest weight. Family matching is
   case-insensitive.
2. If no requested family matched, use the database default face.
3. Append every other registered face as a per-character fallback, skipping
   duplicates of the same physical font file.

`resolve` therefore returns a non-empty ordered list whenever the database has
any face. Shaping selects a face per grapheme cluster: the first face that
contains a glyph for the cluster's first printable character wins, consecutive
clusters using the same face are merged into one run, and `?` is substituted
only when no face can render the cluster.

The first face added to a database becomes its default. Drawen ships with Ubuntu
Light registered under the `"Ubuntu"` family at `FontWeight::LIGHT`:

```rust
use drawen::{FontDatabase, FontFace};

let database = FontDatabase::bundled();
assert_eq!(database.default_face().unwrap().family().as_str(), "Ubuntu");
```

Register additional faces explicitly when their family, weight, or style should
differ from the name embedded in the file:

```rust
use drawen::{FontDatabase, FontDescriptor, FontFace, FontStyle, FontWeight};

let mut database = FontDatabase::bundled();
database.add_font(
    FontDescriptor::new("Inter", FontWeight::BOLD, FontStyle::Italic),
    std::fs::read("assets/Inter-BoldItalic.ttf")?,
)?;
```

`FontFace::from_bytes` infers the family from the font's full name and assumes
normal weight and style. Prefer `from_bytes_with_descriptor` (or
`FontDatabase::add_font`) whenever the file name or metadata does not describe
the face exactly.

## System fonts

`FontDatabase::load_system_fonts()` scans the platform's standard font
directories through `fontdb`; `discover_system_fonts()` adds those faces to an
existing database while preserving its default. This is the portable
enumeration path: it does not depend on `winit`, and applications can still
prefer Drawen's deterministic bundled face with `set_default_face`.

```rust
use drawen::FontDatabase;

let database = FontDatabase::load_system_fonts();
assert!(!database.faces().is_empty());
```

## Shaping

`FontdueTextShaper` wraps a `FontDatabase` and a logical DPI. It resolves the
request in a `TextStyle`, shapes each bidirectional run with HarfRust, and
records one `GlyphRun` per consecutive sequence of glyphs that share a face and
size:

```rust
use drawen::render::{FontdueTextShaper, TextStyle};
use drawen::{Color, Font, FontSize, FontWeight};

let shaper = FontdueTextShaper::ubuntu_light();
let style = TextStyle::new_with_font(
    Font::new()
        .with_family("Ubuntu")
        .with_size(FontSize::pixels(18.0))
        .with_weight(FontWeight::NORMAL),
    Color::BLACK,
);
let layout = shaper.layout("Hello, Drawen", &style, None);

for run in layout.glyph_runs() {
    println!("{} {}px", run.font().family(), run.font_size());
}
```

`TextStyle::new(size, color)` remains the concise constructor for the default
face. `TextStyle::new_with_font` carries a complete request. The selected face
is available from `TextLayout::glyph_runs()`, so callers and tests can verify
exactly which font produced the output.

HarfRust performs complex OpenType shaping: kerning, ligatures, contextual
substitutions, mark positioning, and script-specific behavior. The shaper
splits paragraphs into bidirectional visual runs, shapes each run with the
appropriate direction, applies per-grapheme fallback across registered faces,
wraps at grapheme boundaries, and exposes caret, hit-testing, line, and selection
geometry through `TextLayout` (`caret_x`, `caret_rect`, `byte_at_point`,
`line_at`, `line_at_y`, `line_ranges`, and `selection_rects`). The bounded
layout cache reports hits, misses, and evictions through
`TextShaper::cache_stats()` for frame diagnostics.

To install a custom database, build the shaper and give it to the environment:

```rust
use drawen::{FontdueTextShaper, FontDatabase, UiEnvironment};

let mut database = FontDatabase::bundled();
// register faces...
let shaper = FontdueTextShaper::from_database(database);

let mut environment = UiEnvironment::new(drawen::LightTheme);
environment.set_text_shaper(shaper);
```

The shaper and the environment should agree on DPI. `UiEnvironment` exposes
`logical_dpi()`/`set_logical_dpi()`, and `FontdueTextShaper` exposes
`dpi()`/`set_dpi()`. Point sizes are converted with `points * dpi / 72.0`;
pixel sizes are unaffected.

## Application and per-node defaults

`UiEnvironment` mirrors `QApplication::font()` and the widget-level font
override:

- `default_font()` returns the application font when set, otherwise the
  theme-derived font.
- `set_default_font(font)` installs an application-wide default.
- `clear_default_font()` restores the theme-derived default.
- `font_for(style)` applies a node's DSS font values on top of the default.
- `font_from(base, style)` applies them on top of an arbitrary base request.
- `font_size_for(style)` resolves just the pixel size, including points and DPI.

The `Theme` trait supplies `font_size()`, `font_family()`, `font_weight()`, and
`font_style()`; its `font()` method combines them into a request. A custom theme
can therefore change the default family, and DSS `:root` values override the
theme through `DssTheme`.

Standard widgets shape through the environment instead of hardcoding a face.
`Text::styled(text, style)` defers shaping until layout, so per-node DSS and the
application default font both apply. `Text::wrapping` remains available when a
widget must use an explicit shaper.

## DSS font properties

DSS supports `font-family`, `font-size`, `font-weight`, and `font-style`. All
four inherit from the parent node:

```dss
:root {
    font-family: "Inter", "Noto Sans", sans-serif;
    font-size: 14px;
    font-weight: normal;
}

hint, .caption {
    font-size: 12px;
    font-style: italic;
}

strong {
    font-weight: 700;
}
```

See [`DSS.md`](DSS.md#supported-properties) for the accepted value grammar.

## Current limits

The shaper performs complex OpenType shaping, bidirectional reordering,
grapheme-aware fallback, newlines, width wrapping, and system font enumeration.
It does not implement vertical writing modes, synthetic bold or oblique, color
fonts, or font matching by script metadata; fallback is coverage-driven per
grapheme. Paragraph and line breaking use a grapheme/whitespace heuristic rather
than the full Unicode line-breaking algorithm. These are deliberate boundaries
of the current text contract.
