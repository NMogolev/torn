//! Reproducible headless frame benchmark.
//!
//! Run with `cargo run --release -p drawen --example frame_benchmark`.
//! The output reports p99, never just an average. It intentionally separates
//! retained UI work from software raster work: a GPU comparison needs a native
//! surface and must be collected by the platform-specific benchmark described
//! in `docs/PERFORMANCE.md`.

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use drawen::{
    Color, Constraints, InputEvent, Modifiers, Point, Rect, Size, UiRuntime, VirtualDataSource,
    VirtualList, WheelDelta, WheelEvent, Widget,
    render::{DisplayList, PaintContext},
    software::{PixelBuffer, SoftwareRenderer},
};

const SAMPLES: usize = 240;
const WINDOW_WIDTH: f32 = 1_280.0;
const WINDOW_HEIGHT: f32 = 800.0;

fn main() {
    println!("Drawen performance benchmark ({SAMPLES} measured frames; p99 after warmup)\n");
    retained_frame_benchmark();
    software_fill_benchmark(1_920, 1_080);
    software_fill_benchmark(3_840, 2_160);
    virtual_list_scroll_benchmark();
}

fn retained_frame_benchmark() {
    let size = fixed_size(WINDOW_WIDTH, WINDOW_HEIGHT);
    let mut runtime = UiRuntime::new(FillScene);
    layout(&mut runtime, size);

    // Build the retained display list once. Subsequent frames must not repaint it.
    paint_frame(&mut runtime);
    let samples = measure(|| {
        paint_frame(&mut runtime);
    });
    let diagnostics = runtime.frame_diagnostics();

    println!("unchanged retained frame");
    print_timing(&samples);
    println!(
        "  retained paint {}  damage {}  commands {}",
        diagnostics.last_paint_reused(),
        describe_damage(diagnostics.last_damage()),
        diagnostics.last_paint_command_count(),
    );
    println!("  note: this includes appending the retained list to a fresh caller output list\n");
}

fn software_fill_benchmark(width: u32, height: u32) {
    let mut list = DisplayList::new();
    PaintContext::new(&mut list).fill_rect(
        Rect::new(
            Point::ZERO,
            fixed_size(u32_to_f32(width), u32_to_f32(height)),
        ),
        Color::rgba8(32, 96, 160, 255),
    );
    let renderer = SoftwareRenderer;
    let mut pixels =
        PixelBuffer::new(width, height).expect("benchmark buffer dimensions are valid");
    renderer.render(&list, &mut pixels).expect("fill renders");
    let samples = measure(|| {
        renderer.render(&list, &mut pixels).expect("fill renders");
    });
    let median = percentile(&samples, 50);
    let pixels_per_second = f64::from(width) * f64::from(height) / median.as_secs_f64();

    println!("software fill {width}x{height}");
    print_timing(&samples);
    println!(
        "  median fill rate {:.2} Gpixel/s\n",
        pixels_per_second / 1e9
    );
}

fn virtual_list_scroll_benchmark() {
    let size = fixed_size(WINDOW_WIDTH, WINDOW_HEIGHT);
    let mut runtime = UiRuntime::new(VirtualList::new(BenchmarkRows(1_000_000), 28.0));
    layout(&mut runtime, size);
    paint_frame(&mut runtime);

    let mut text_commands = 0;
    let samples = measure(|| {
        let scroll = InputEvent::Wheel(WheelEvent {
            position: Point::new(10.0, 10.0),
            delta: WheelDelta::Pixels(Point::new(0.0, -28.0)),
            modifiers: Modifiers::NONE,
        });
        runtime.dispatch_event(&scroll);
        layout(&mut runtime, size);
        let list = paint_frame(&mut runtime);
        text_commands = list
            .commands()
            .iter()
            .filter(|command| matches!(command, drawen::render::DisplayCommand::DrawText { .. }))
            .count();
    });

    println!("virtual list scroll (1,000,000 rows)");
    print_timing(&samples);
    println!(
        "  visible row text commands {text_commands} (must scale with viewport, not row count)\n"
    );
}

fn layout(runtime: &mut UiRuntime, size: Size) {
    runtime
        .layout(Constraints::tight(size).expect("benchmark constraints are valid"))
        .expect("benchmark scene lays out");
}

fn paint_frame(runtime: &mut UiRuntime) -> DisplayList {
    let mut display_list = DisplayList::new();
    runtime
        .paint(&mut PaintContext::new(&mut display_list))
        .expect("benchmark scene paints");
    black_box(display_list.len());
    display_list
}

fn measure(mut frame: impl FnMut()) -> Vec<Duration> {
    for _ in 0..16 {
        frame();
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        frame();
        samples.push(started.elapsed());
    }
    samples
}

fn print_timing(samples: &[Duration]) {
    println!(
        "  median {:>8.2} us  p99 {:>8.2} us  worst {:>8.2} us",
        microseconds(percentile(samples, 50)),
        microseconds(percentile(samples, 99)),
        microseconds(*samples.iter().max().expect("samples are non-empty")),
    );
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = (percentile * (sorted.len() - 1)).div_ceil(100);
    sorted[index]
}

fn describe_damage(damage: Option<Rect>) -> &'static str {
    if damage.is_some() { "present" } else { "none" }
}

fn microseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000_000.0
}

struct FillScene;

impl Widget for FillScene {
    fn layout(
        &mut self,
        _: &mut drawen::LayoutContext<'_>,
        constraints: Constraints,
    ) -> drawen::LayoutResult {
        drawen::LayoutResult::new(constraints.constrain(fixed_size(WINDOW_WIDTH, WINDOW_HEIGHT)))
    }

    fn style_target(&self) -> drawen::StyleTarget {
        drawen::StyleTarget::new("benchmark-fill")
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &drawen::UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, Color::rgba8(32, 96, 160, 255));
    }
}

struct BenchmarkRows(usize);

impl VirtualDataSource for BenchmarkRows {
    fn row_count(&self) -> usize {
        self.0
    }

    fn row_key(&self, row: usize) -> u64 {
        row as u64
    }

    fn row_text(&self, row: usize) -> String {
        format!("Item {row:06} — cached row visual")
    }
}

fn fixed_size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("benchmark dimensions are finite")
}

fn u32_to_f32(value: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}
