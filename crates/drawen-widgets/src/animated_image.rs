use std::{fmt, io::Cursor, time::Duration};

use drawen_core::{Color, Constraints, Rect, Size};
use drawen_render::{PaintContext, RasterImage};
use drawen_ui::{
    Accessibility, AccessibilityRole, DssStyle, LayoutContext, LayoutResult, StyleTarget,
    UiEnvironment, Widget,
};

use crate::image::{fitted_rect, image_size};
use crate::media::duration_remainder;
use crate::{Alignment, ImageFit};

const MIN_FRAME_DELAY: Duration = Duration::from_millis(10);
const DEFAULT_GIF_DELAY: Duration = Duration::from_millis(100);

/// One frame of an [`AnimatedImage`] and how long it stays on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationFrame {
    image: RasterImage,
    duration: Duration,
}

impl AnimationFrame {
    /// Creates a frame shown for `duration`.
    ///
    /// Delays shorter than 10 milliseconds are clamped so playback can never
    /// spin without advancing time.
    #[must_use]
    pub fn new(image: RasterImage, duration: Duration) -> Self {
        Self {
            image,
            duration: duration.max(MIN_FRAME_DELAY),
        }
    }

    /// Returns the frame pixels.
    #[must_use]
    pub const fn image(&self) -> &RasterImage {
        &self.image
    }

    /// Returns how long the frame stays on screen.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }
}

/// A widget that plays a sequence of raster frames.
///
/// The runtime advances animation-driven widgets before painting, so a playing
/// `AnimatedImage` keeps requesting frames through
/// [`drawen_ui::UiRuntime::next_frame_in`]. Use [`Self::from_gif`] to decode an
/// animated GIF into frames.
pub struct AnimatedImage {
    frames: Vec<AnimationFrame>,
    current: usize,
    elapsed: Duration,
    playing: bool,
    looping: bool,
    finished: bool,
    display_size: Option<Size>,
    fit: ImageFit,
    horizontal: Alignment,
    vertical: Alignment,
    background: Option<Color>,
    label: Option<String>,
}

impl AnimatedImage {
    /// Creates a playing animation from `frames`.
    ///
    /// A single-frame animation starts paused because there is nothing to
    /// animate.
    #[must_use]
    pub fn new(frames: Vec<AnimationFrame>) -> Self {
        let playing = frames.len() > 1;
        Self {
            frames,
            current: 0,
            elapsed: Duration::ZERO,
            playing,
            looping: true,
            finished: false,
            display_size: None,
            fit: ImageFit::Contain,
            horizontal: Alignment::Center,
            vertical: Alignment::Center,
            background: None,
            label: None,
        }
    }

    /// Creates an empty animation with nothing to paint.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    /// Decodes an animated GIF.
    ///
    /// Frames are composited onto the logical screen by the decoder, so each
    /// [`AnimationFrame`] contains a complete image and can be painted without
    /// additional blending work.
    ///
    /// # Errors
    ///
    /// Returns [`GifDecodeError`] when the file is not a valid GIF, contains no
    /// frames, or declares dimensions that cannot be represented in memory.
    pub fn from_gif(bytes: &[u8]) -> Result<Self, GifDecodeError> {
        use image::AnimationDecoder;

        let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .map_err(|error| GifDecodeError::new(error.to_string()))?;
        let gif_frames = decoder
            .into_frames()
            .collect_frames()
            .map_err(|error| GifDecodeError::new(error.to_string()))?;
        if gif_frames.is_empty() {
            return Err(GifDecodeError::new("GIF contains no frames"));
        }

        let mut frames = Vec::with_capacity(gif_frames.len());
        for frame in gif_frames {
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            let duration = if numerator == 0 || denominator == 0 {
                DEFAULT_GIF_DELAY
            } else {
                Duration::from_millis(u64::from(numerator) / u64::from(denominator))
            };
            let buffer = frame.into_buffer();
            let (width, height) = buffer.dimensions();
            let image = RasterImage::new(width, height, buffer.into_raw())
                .map_err(|error| GifDecodeError::new(error.to_string()))?;
            frames.push(AnimationFrame::new(image, duration));
        }
        Ok(Self::new(frames))
    }

    /// Returns the frames in playback order.
    #[must_use]
    pub fn frames(&self) -> &[AnimationFrame] {
        &self.frames
    }

    /// Returns the number of frames.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Returns the frame currently selected for painting.
    #[must_use]
    pub fn current_frame(&self) -> Option<&AnimationFrame> {
        self.frames.get(self.current)
    }

    /// Returns the index of the frame currently selected for painting.
    #[must_use]
    pub const fn current_index(&self) -> usize {
        self.current
    }

    /// Returns whether playback is running.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    /// Returns whether playback wraps around after the last frame.
    #[must_use]
    pub const fn is_looping(&self) -> bool {
        self.looping
    }

    /// Returns whether a non-looping animation reached its last frame.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns how the frame is scaled into the widget bounds.
    #[must_use]
    pub const fn fit(&self) -> ImageFit {
        self.fit
    }

    /// Returns the optional accessible name.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns this animation with an explicit logical display size.
    ///
    /// # Panics
    ///
    /// Panics when either dimension is negative or NaN.
    #[must_use]
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.display_size = Some(
            Size::new(width, height).expect("animated image sizes are non-negative and not NaN"),
        );
        self
    }

    /// Returns this animation with an explicit scaling mode.
    #[must_use]
    pub const fn with_fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    /// Returns this animation with explicit horizontal and vertical alignment.
    #[must_use]
    pub const fn with_alignment(mut self, horizontal: Alignment, vertical: Alignment) -> Self {
        self.horizontal = horizontal;
        self.vertical = vertical;
        self
    }

    /// Returns this animation with a background painted behind transparent pixels.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Returns this animation with looping enabled or disabled.
    #[must_use]
    pub const fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Returns this animation playing or paused.
    #[must_use]
    pub const fn with_playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    /// Returns this animation with an accessible name.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Starts or resumes playback.
    ///
    /// Playback restarts from the first frame when a finished non-looping
    /// animation is played again.
    pub fn play(&mut self) {
        if self.frames.is_empty() {
            return;
        }
        if self.finished {
            self.current = 0;
            self.elapsed = Duration::ZERO;
            self.finished = false;
        }
        self.playing = self.frames.len() > 1;
    }

    /// Pauses playback at the current frame.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Restarts playback from the first frame.
    pub fn restart(&mut self) {
        if self.frames.is_empty() {
            return;
        }
        self.current = 0;
        self.elapsed = Duration::ZERO;
        self.finished = false;
        self.playing = self.frames.len() > 1;
    }

    /// Selects a frame and pauses playback.
    ///
    /// The index is clamped to the available frame range.
    pub fn select_frame(&mut self, index: usize) {
        if self.frames.is_empty() {
            return;
        }
        self.current = index.min(self.frames.len() - 1);
        self.elapsed = Duration::ZERO;
        self.finished = false;
        self.playing = false;
    }

    /// Replaces the frame sequence.
    pub fn set_frames(&mut self, frames: Vec<AnimationFrame>) {
        self.frames = frames;
        self.current = 0;
        self.elapsed = Duration::ZERO;
        self.finished = false;
        self.playing = self.frames.len() > 1;
    }

    fn duration_of(&self, index: usize) -> Duration {
        self.frames
            .get(index)
            .map_or(MIN_FRAME_DELAY, AnimationFrame::duration)
    }

    fn advance(&mut self, delta: Duration) -> bool {
        if self.frames.is_empty() || !self.playing || self.finished {
            return false;
        }
        let total = self
            .frames
            .iter()
            .fold(Duration::ZERO, |total, frame| total + frame.duration());
        if self.looping && total > Duration::ZERO {
            self.elapsed = duration_remainder(self.elapsed.saturating_add(delta), total);
        } else {
            self.elapsed = self.elapsed.saturating_add(delta);
        }

        let mut changed = false;
        loop {
            let duration = self.duration_of(self.current);
            if self.elapsed < duration {
                break;
            }
            self.elapsed -= duration;
            if self.current + 1 < self.frames.len() {
                self.current += 1;
                changed = true;
            } else if self.looping {
                self.current = 0;
                changed = true;
            } else {
                self.current = self.frames.len() - 1;
                self.elapsed = Duration::ZERO;
                self.playing = false;
                self.finished = true;
                changed = true;
                break;
            }
        }
        changed
    }
}

impl Default for AnimatedImage {
    fn default() -> Self {
        Self::empty()
    }
}

impl Widget for AnimatedImage {
    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("animated-image")
    }

    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let intrinsic = self
            .current_frame()
            .map_or(Size::ZERO, |frame| image_size(frame.image()));
        LayoutResult::new(constraints.constrain(self.display_size.unwrap_or(intrinsic)))
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
        let _ = environment;
        if let Some(background) = self.background.or(style.background) {
            context.fill_rect(bounds, background);
        }
        let Some(frame) = self.current_frame() else {
            return;
        };
        let image = frame.image();
        if image.width() == 0 || image.height() == 0 || bounds.is_empty() {
            return;
        }
        let destination = fitted_rect(
            bounds,
            image_size(image),
            self.fit,
            self.horizontal,
            self.vertical,
        );
        context.with_clip(bounds, |context| {
            context.draw_image(image.clone(), destination);
        });
    }

    fn advance_animation(&mut self, delta: Duration, _settle: bool) -> bool {
        self.advance(delta)
    }

    fn is_animating(&self) -> bool {
        self.playing && !self.finished && self.frames.len() > 1
    }

    fn participates_in_hit_test(&self) -> bool {
        false
    }

    fn accessibility(&self) -> Accessibility {
        let accessibility = Accessibility::new(AccessibilityRole::Image);
        match &self.label {
            Some(label) => accessibility.with_label(label.clone()),
            None => accessibility,
        }
    }
}

/// Why decoding an animated GIF failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GifDecodeError {
    message: String,
}

impl GifDecodeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the decoder's human-readable explanation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for GifDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "could not decode GIF: {}", self.message)
    }
}

impl std::error::Error for GifDecodeError {}

#[cfg(test)]
mod tests {
    use drawen_render::DisplayList;
    use drawen_ui::UiRuntime;
    use image::{Delay, Frame, Rgba, RgbaImage, codecs::gif::GifEncoder};

    use super::*;

    fn frame(red: u8) -> AnimationFrame {
        AnimationFrame::new(
            RasterImage::new(2, 2, [red, 0, 0, 255].repeat(4)).expect("valid RGBA image"),
            Duration::from_millis(100),
        )
    }

    #[test]
    fn advances_and_wraps_frames() {
        let mut animation = AnimatedImage::new(vec![frame(1), frame(2), frame(3)]);
        assert_eq!(animation.current_index(), 0);
        assert!(animation.advance(Duration::from_millis(100)));
        assert_eq!(animation.current_index(), 1);
        assert!(animation.advance(Duration::from_millis(200)));
        assert_eq!(animation.current_index(), 0);
        assert!(animation.is_animating());
    }

    #[test]
    fn non_looping_animations_finish_on_the_last_frame() {
        let mut animation = AnimatedImage::new(vec![frame(1), frame(2)]).with_looping(false);
        animation.advance(Duration::from_millis(100));
        assert!(animation.advance(Duration::from_millis(100)));
        assert_eq!(animation.current_index(), 1);
        assert!(animation.is_finished());
        assert!(!animation.is_playing());
        assert!(!animation.is_animating());
    }

    #[test]
    fn play_restarts_a_finished_animation() {
        let mut animation = AnimatedImage::new(vec![frame(1), frame(2)]).with_looping(false);
        animation.advance(Duration::from_millis(200));
        assert!(animation.is_finished());
        animation.play();
        assert_eq!(animation.current_index(), 0);
        assert!(!animation.is_finished());
        assert!(animation.is_playing());
    }

    #[test]
    fn decodes_a_gif_into_composited_frames() {
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])),
                    0,
                    0,
                    Delay::from_numer_denom_ms(50, 1),
                ))
                .expect("first frame encodes");
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(2, 2, Rgba([0, 255, 0, 255])),
                    0,
                    0,
                    Delay::from_numer_denom_ms(50, 1),
                ))
                .expect("second frame encodes");
        }

        let animation = AnimatedImage::from_gif(&bytes).expect("GIF decodes");
        assert_eq!(animation.frame_count(), 2);
        assert_eq!(
            animation
                .current_frame()
                .expect("first frame exists")
                .image()
                .pixels()[0],
            255
        );
    }

    #[test]
    fn paints_the_current_frame_in_contain_mode() {
        let animation = AnimatedImage::new(vec![frame(255)]);
        let mut runtime = UiRuntime::new(animation);
        let mut list = DisplayList::new();
        runtime
            .layout(
                Constraints::tight(Size::new(10.0, 10.0).expect("valid size"))
                    .expect("valid constraints"),
            )
            .expect("animation layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("animation paint succeeds");
        assert!(list.commands().iter().any(|command| matches!(
            command,
            drawen_render::DisplayCommand::DrawImage { destination, .. }
                if destination.size == Size::new(10.0, 10.0).expect("valid size")
        )));
    }
}
