//! Headless frame benchmark for render diagnostics and virtualized controls.
//!
//! Run with `cargo run --release -p torn --example frame_benchmark`.
//! Each scene paints a fixed number of frames and prints the counters from
//! [`torn::FrameDiagnostics`]: layout and paint timing, cache reuse, display
//! command composition, damage coverage, and text-layout cache activity.

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use torn::{
    Color, Constraints, FrameDiagnostics, Point, Rect, Size, UiRuntime, VirtualDataSource,
    VirtualList, Widget,
    render::{FontdueTextShaper, PaintContext, TextStyle},
};

const FRAMES: u32 = 60;
const WINDOW_WIDTH: f32 = 1_280.0;
const WINDOW_HEIGHT: f32 = 800.0;

fn main() {
    let size = fixed_size(WINDOW_WIDTH, WINDOW_HEIGHT);
    println!("Torn frame benchmark: {WINDOW_WIDTH}x{WINDOW_HEIGHT}, {FRAMES} frames per scene\n");
    run_scene("fill commands", size, FillScene::new(80, 40));
    run_scene("text shaping", size, TextScene::new(20, 12));
    run_scene("virtual list", size, virtual_list_scene());
}

fn run_scene(label: &str, size: Size, scene: impl Widget + 'static) {
    let mut runtime = UiRuntime::new(scene);
    runtime
        .layout(Constraints::tight(size).expect("benchmark constraints are valid"))
        .expect("benchmark scene lays out");

    // Warm caches before timing so the report focuses on steady-state cost.
    for _ in 0..3 {
        paint_frame(&mut runtime);
    }

    let mut total = Duration::ZERO;
    let mut diagnostics = FrameDiagnostics::default();
    for _ in 0..FRAMES {
        let started = Instant::now();
        paint_frame(&mut runtime);
        total += started.elapsed();
        diagnostics = runtime.frame_diagnostics();
    }
    let average = total / FRAMES;
    let display = diagnostics.last_display_stats();

    println!("{label}");
    println!(
        "  paint/total  {:>9.2} ms avg  (last {:>9.2} ms)",
        milliseconds(average),
        milliseconds(diagnostics.last_paint_time())
    );
    println!(
        "  commands     {:>9}  fills {}  rounded {}  strokes {}  text {}  clips {}  state {}",
        diagnostics.last_paint_command_count(),
        display.fills,
        display.rounded_fills,
        display.strokes + display.rounded_strokes,
        display.text_runs,
        display.clips,
        display.state_changes
    );
    println!(
        "  caches       paint reused {}  layout reused {}  text {} hits / {} misses ({} entries)",
        diagnostics.last_paint_reused(),
        diagnostics.last_layout_reused(),
        diagnostics.last_text_cache().hits,
        diagnostics.last_text_cache().misses,
        diagnostics.last_text_cache().entries
    );
    println!(
        "  damage       {}\n",
        describe_damage(diagnostics.last_damage())
    );
}

fn paint_frame(runtime: &mut UiRuntime) {
    runtime.invalidate_paint();
    let mut display_list = torn::render::DisplayList::new();
    runtime
        .paint(&mut PaintContext::new(&mut display_list))
        .expect("benchmark scene paints");
    black_box(display_list.len());
}

/// Paints a dense grid of filled rectangles and rounded rectangles.
struct FillScene {
    columns: usize,
    rows: usize,
}

impl FillScene {
    const fn new(columns: usize, rows: usize) -> Self {
        Self { columns, rows }
    }
}

impl Widget for FillScene {
    fn layout(
        &mut self,
        _: &mut torn::LayoutContext<'_>,
        constraints: Constraints,
    ) -> torn::LayoutResult {
        torn::LayoutResult::new(constraints.constrain(fixed_size(WINDOW_WIDTH, WINDOW_HEIGHT)))
    }

    fn style_target(&self) -> torn::StyleTarget {
        torn::StyleTarget::new("benchmark-fill")
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &torn::UiEnvironment, bounds: Rect) {
        let cell_width = bounds.size.width() / usize_to_f32(self.columns);
        let cell_height = bounds.size.height() / usize_to_f32(self.rows);
        for row in 0..self.rows {
            for column in 0..self.columns {
                let x = bounds.origin.x + cell_width * usize_to_f32(column);
                let y = bounds.origin.y + cell_height * usize_to_f32(row);
                let rect = Rect::new(
                    Point::new(x, y),
                    fixed_size(cell_width * 0.8, cell_height * 0.8),
                );
                let shade = (row * self.columns + column) % 255;
                let color = Color::rgba8(u8::try_from(shade).unwrap_or(0), 96, 160, 255);
                if shade.is_multiple_of(3) {
                    context.fill_rounded_rect(rect, cell_height * 0.2, color);
                } else {
                    context.fill_rect(rect, color);
                }
            }
        }
    }
}

/// Reshapes a grid of distinct strings through the shared text-layout cache.
struct TextScene {
    labels: Vec<String>,
    shaper: FontdueTextShaper,
    style: TextStyle,
    columns: usize,
    rows: usize,
}

impl TextScene {
    fn new(columns: usize, rows: usize) -> Self {
        let labels = (0..columns * rows)
            .map(|index| format!("Row {index:04} — العربية עברית ligature fi"))
            .collect();
        Self {
            labels,
            shaper: FontdueTextShaper::ubuntu_light(),
            style: TextStyle::new(15.0, Color::WHITE),
            columns,
            rows,
        }
    }
}

impl Widget for TextScene {
    fn layout(
        &mut self,
        _: &mut torn::LayoutContext<'_>,
        constraints: Constraints,
    ) -> torn::LayoutResult {
        torn::LayoutResult::new(constraints.constrain(fixed_size(WINDOW_WIDTH, WINDOW_HEIGHT)))
    }

    fn style_target(&self) -> torn::StyleTarget {
        torn::StyleTarget::new("benchmark-text")
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &torn::UiEnvironment, bounds: Rect) {
        let cell_width = bounds.size.width() / usize_to_f32(self.columns);
        let cell_height = bounds.size.height() / usize_to_f32(self.rows);
        for row in 0..self.rows {
            for column in 0..self.columns {
                let Some(label) = self
                    .labels
                    .get(row * self.columns + column)
                    .map(String::as_str)
                else {
                    continue;
                };
                let layout = self.shaper.layout(label, &self.style, Some(cell_width));
                let origin = Point::new(
                    bounds.origin.x + cell_width * usize_to_f32(column),
                    bounds.origin.y + cell_height * usize_to_f32(row),
                );
                context.draw_text(layout, origin);
            }
        }
    }
}

fn virtual_list_scene() -> VirtualList<BenchmarkRows> {
    VirtualList::new(BenchmarkRows(100_000), 28.0)
}

struct BenchmarkRows(usize);

impl VirtualDataSource for BenchmarkRows {
    fn row_count(&self) -> usize {
        self.0
    }

    fn row_key(&self, row: usize) -> u64 {
        u64::try_from(row).unwrap_or(u64::MAX)
    }

    fn row_text(&self, row: usize) -> String {
        format!("Item {row:06} — cached row visual")
    }

    fn label(&self) -> Option<String> {
        Some("Benchmark rows".to_owned())
    }
}

fn describe_damage(damage: Option<Rect>) -> String {
    match damage {
        Some(rect) => format!(
            "{:.0}x{:.0} at ({:.0}, {:.0})",
            rect.size.width(),
            rect.size.height(),
            rect.origin.x,
            rect.origin.y
        ),
        None => "none (retained output reused)".to_owned(),
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn fixed_size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("benchmark dimensions are finite")
}

fn usize_to_f32(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}
