//! Headless raster benchmark for the software renderer's scanline paths.
//!
//! Run with `cargo run --release -p drawen-software --example fill_benchmark`.
//! It measures only fill rasterization, not retained UI work; use the
//! `frame_benchmark` example in the `drawen` facade for end-to-end frames.

use std::hint::black_box;
use std::time::{Duration, Instant};

use drawen_core::{Color, Point, Rect, Size};
use drawen_render::{DisplayList, PaintContext};
use drawen_software::{PixelBuffer, SoftwareRenderer};

const SAMPLES: usize = 120;

fn main() {
    println!(
        "DisplayCommand = {} B, RasterImage = {} B, TextLayout = {} B\n",
        std::mem::size_of::<drawen_render::DisplayCommand>(),
        std::mem::size_of::<drawen_render::RasterImage>(),
        std::mem::size_of::<drawen_render::TextLayout>(),
    );
    for (width, height) in [(1_920, 1_080), (3_840, 2_160)] {
        benchmark(width, height);
    }
}

fn benchmark(width: u32, height: u32) {
    let full = Rect::new(
        Point::ZERO,
        Size::new(as_f32(width), as_f32(height)).expect("valid size"),
    );

    let mut opaque = DisplayList::new();
    PaintContext::new(&mut opaque).fill_rect(full, Color::rgba8(32, 96, 160, 255));

    let mut translucent = DisplayList::new();
    PaintContext::new(&mut translucent).fill_rect(full, Color::rgba8(32, 96, 160, 128));

    let mut rounded = DisplayList::new();
    PaintContext::new(&mut rounded).fill_rounded_rect(full, 32.0, Color::rgba8(32, 96, 160, 255));

    let mut clipped = DisplayList::new();
    PaintContext::new(&mut clipped).with_clip(
        Rect::new(
            Point::ZERO,
            Size::new(as_f32(width) / 2.0, as_f32(height)).expect("valid size"),
        ),
        |context| context.fill_rounded_rect(full, 32.0, Color::rgba8(32, 96, 160, 255)),
    );

    let mut pixels = PixelBuffer::new(width, height).expect("valid buffer");
    let mut measure = |name: &str, list: &DisplayList| {
        let renderer = SoftwareRenderer;
        renderer.render(list, &mut pixels).expect("render");
        for _ in 0..8 {
            renderer.render(list, &mut pixels).expect("warmup");
        }
        let mut samples = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = Instant::now();
            renderer.render(list, &mut pixels).expect("render");
            black_box(&pixels);
            samples.push(started.elapsed());
        }
        samples.sort_unstable();
        let median = samples[SAMPLES / 2];
        let p99 = samples[SAMPLES - 1 - SAMPLES / 100];
        let pixels_per_second = f64::from(width) * f64::from(height) / median.as_secs_f64();
        println!(
            "{width}x{height} {name}: median {:.3} ms ({:.2} Gpixel/s, {:.2} ns/px), p99 {:.3} ms",
            millis(median),
            pixels_per_second / 1e9,
            median.as_secs_f64() * 1e9 / (f64::from(width) * f64::from(height)),
            millis(p99)
        );
    };

    measure("opaque fill", &opaque);
    measure("translucent fill", &translucent);
    measure("rounded fill r=32", &rounded);
    measure("half-clipped rounded", &clipped);
    println!();
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn as_f32(value: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}
