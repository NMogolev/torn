use std::path::Path;
use std::time::Duration;

use torn_core::{
    Color, Constraints, InputEvent, Key, NamedKey, Point, PointerButton, PointerId, Rect, Size,
};
use torn_render::{PaintContext, RasterImage, TextStyle};
use torn_ui::{
    Accessibility, AccessibilityRole, EventContext, EventPhase, EventStatus, LayoutContext,
    LayoutResult, Signal, StyleTarget, TssStyle, UiEnvironment, Widget,
};

use crate::ImageFit;
use crate::image::{fitted_rect, image_size};
use crate::media::duration_remainder;
use crate::video_ffmpeg::{FfmpegConfig, FfmpegDecoder, VideoError, VideoMetadata};

const DEFAULT_WIDTH: f32 = 480.0;
const DEFAULT_HEIGHT: f32 = 270.0;
const CONTROL_HEIGHT: f32 = 40.0;
const CONTROL_PADDING: f32 = 10.0;
const PLAY_BUTTON: f32 = 24.0;
const PROGRESS_HEIGHT: f32 = 4.0;
const TIME_RESERVE: f32 = 96.0;
const SEEK_STEP_SECONDS: f32 = 5.0;

/// A video presentation surface with optional `FFmpeg` decoding.
///
/// [`Self::open`] starts the built-in `FFmpeg` command-line backend, which probes
/// metadata, decodes frames on a worker thread, and keeps the displayed frame
/// in sync with playback. Applications that already own a decoder can keep
/// using [`Self::set_frame`] and react to [`Self::play_toggled`] and
/// [`Self::seek_requested`] instead. While playing, the widget advances its own
/// position so progress and time labels stay live; externally supplied frames
/// should arrive at the matching rate.
pub struct Video {
    frame: Option<RasterImage>,
    duration: Duration,
    position: Duration,
    playing: bool,
    looping: bool,
    controls: bool,
    fit: ImageFit,
    background: Option<Color>,
    label: Option<String>,
    viewport: Size,
    seeking: Option<PointerId>,
    decoder: Option<FfmpegDecoder>,
    metadata: Option<VideoMetadata>,
    decoder_error: Option<String>,
    play_toggled: Signal<bool>,
    seek_requested: Signal<Duration>,
}

impl Video {
    /// Creates an empty video surface with controls enabled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            frame: None,
            duration: Duration::ZERO,
            position: Duration::ZERO,
            playing: false,
            looping: false,
            controls: true,
            fit: ImageFit::Contain,
            background: None,
            label: None,
            viewport: Size::ZERO,
            seeking: None,
            decoder: None,
            metadata: None,
            decoder_error: None,
            play_toggled: Signal::new(),
            seek_requested: Signal::new(),
        }
    }

    /// Creates a video surface showing `frame`.
    #[must_use]
    pub fn with_frame(frame: RasterImage) -> Self {
        let mut video = Self::new();
        video.frame = Some(frame);
        video
    }

    /// Opens `path` with the built-in `FFmpeg` backend.
    ///
    /// This starts decoding immediately so the first frame can be shown while
    /// playback is paused. Playback, seeking, and looping then stay in sync
    /// with the decoder until the widget is dropped or [`Self::close_decoder`]
    /// is called.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError`] when `ffprobe` or `ffmpeg` cannot be found, the
    /// container is rejected, or the decoded size cannot be represented.
    /// Both executables are discovered through [`FfmpegConfig`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VideoError> {
        Self::open_with(path, FfmpegConfig::default())
    }

    /// Opens `path` with explicit `FFmpeg` settings.
    ///
    /// # Errors
    ///
    /// Returns [`VideoError`] when `ffprobe` or `ffmpeg` cannot be found, the
    /// container is rejected, or the decoded size cannot be represented.
    pub fn open_with(path: impl AsRef<Path>, config: FfmpegConfig) -> Result<Self, VideoError> {
        let (decoder, metadata) = FfmpegDecoder::open(path.as_ref(), config)?;
        let mut video = Self::new();
        video.duration = metadata.duration();
        video.metadata = Some(metadata);
        video.decoder = Some(decoder);
        Ok(video)
    }

    /// Returns the metadata reported by the built-in decoder.
    #[must_use]
    pub const fn metadata(&self) -> Option<VideoMetadata> {
        self.metadata
    }

    /// Returns whether the built-in `FFmpeg` decoder is attached.
    #[must_use]
    pub const fn has_decoder(&self) -> bool {
        self.decoder.is_some()
    }

    /// Returns the most recent decoder error, if any.
    ///
    /// Decoding happens on a worker thread, so start-up and read failures are
    /// reported here rather than by [`Self::open`]. A reported error stops
    /// playback immediately.
    #[must_use]
    pub fn decoder_error(&self) -> Option<&str> {
        self.decoder_error.as_deref()
    }

    /// Stops and releases the built-in decoder while keeping the last frame.
    pub fn close_decoder(&mut self) {
        self.decoder = None;
        self.metadata = None;
        self.decoder_error = None;
        self.playing = false;
        self.position = Duration::ZERO;
    }

    /// Returns the current frame, if one has been supplied.
    #[must_use]
    pub const fn frame(&self) -> Option<&RasterImage> {
        self.frame.as_ref()
    }

    /// Returns the declared media duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns the current playback position.
    #[must_use]
    pub const fn position(&self) -> Duration {
        self.position
    }

    /// Returns whether playback is currently running.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    /// Returns whether playback wraps to the start at the end.
    #[must_use]
    pub const fn is_looping(&self) -> bool {
        self.looping
    }

    /// Returns whether the overlay controls are painted.
    #[must_use]
    pub const fn controls_visible(&self) -> bool {
        self.controls
    }

    /// Returns how frames are scaled into the widget bounds.
    #[must_use]
    pub const fn fit(&self) -> ImageFit {
        self.fit
    }

    /// Returns the optional accessible name.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns a signal emitted when the user starts or pauses playback.
    #[must_use]
    pub fn play_toggled(&self) -> Signal<bool> {
        self.play_toggled.clone()
    }

    /// Returns a signal emitted when the user seeks to a new position.
    #[must_use]
    pub fn seek_requested(&self) -> Signal<Duration> {
        self.seek_requested.clone()
    }

    /// Replaces the displayed frame without changing playback state.
    pub fn set_frame(&mut self, frame: RasterImage) {
        self.frame = Some(frame);
    }

    /// Clears the displayed frame.
    pub fn clear_frame(&mut self) {
        self.frame = None;
    }

    /// Sets the media duration and clamps the current position to it.
    pub fn set_duration(&mut self, duration: Duration) {
        self.duration = duration;
        if !duration.is_zero() {
            self.position = self.position.min(duration);
        }
    }

    /// Sets the playback position, clamping it to `0..=duration`.
    ///
    /// With an attached decoder this seeks immediately. Seeking while paused
    /// asks the decoder for a preview frame at the new position.
    pub fn set_position(&mut self, position: Duration) {
        self.position = if self.duration.is_zero() {
            position
        } else {
            position.min(self.duration)
        };
        if let Some(decoder) = &self.decoder {
            decoder.seek(self.position, self.playing);
        }
    }

    /// Updates playback state without emitting [`Self::play_toggled`].
    pub fn set_playing(&mut self, playing: bool) {
        if playing && !self.looping && !self.duration.is_zero() && self.position >= self.duration {
            self.position = Duration::ZERO;
        }
        self.playing = playing;
        self.sync_decoder();
    }

    /// Starts or pauses playback without emitting [`Self::play_toggled`].
    pub fn toggle_playback(&mut self) {
        self.set_playing(!self.playing);
    }

    fn sync_decoder(&self) {
        let Some(decoder) = &self.decoder else {
            return;
        };
        if self.playing {
            decoder.play_at(self.position);
        } else {
            decoder.pause();
        }
    }

    fn poll_decoder(&mut self) -> bool {
        let Some(decoder) = &self.decoder else {
            return false;
        };
        let mut changed = false;
        if let Some(frame) = decoder.take_frame() {
            self.frame = Some(frame);
            changed = true;
        }
        if let Some(error) = decoder.take_error() {
            self.decoder_error = Some(error);
            self.playing = false;
            decoder.pause();
            changed = true;
        }
        if decoder.take_eof() {
            if self.looping {
                self.position = Duration::ZERO;
                if self.playing {
                    decoder.play_at(Duration::ZERO);
                } else {
                    decoder.preview(Duration::ZERO);
                }
            } else {
                if !self.duration.is_zero() {
                    self.position = self.duration;
                }
                self.playing = false;
            }
            changed = true;
        }
        changed
    }

    /// Returns this video with an explicit duration.
    #[must_use]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.set_duration(duration);
        self
    }

    /// Returns this video with an initial position.
    #[must_use]
    pub fn with_position(mut self, position: Duration) -> Self {
        self.set_position(position);
        self
    }

    /// Returns this video playing or paused.
    #[must_use]
    pub fn with_playing(mut self, playing: bool) -> Self {
        self.set_playing(playing);
        self
    }

    /// Returns this video with looping enabled or disabled.
    #[must_use]
    pub const fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Returns this video with overlay controls shown or hidden.
    #[must_use]
    pub const fn with_controls(mut self, controls: bool) -> Self {
        self.controls = controls;
        self
    }

    /// Returns this video with an explicit frame scaling mode.
    #[must_use]
    pub const fn with_fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    /// Returns this video with a letterbox background color.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Returns this video with an accessible name.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    fn toggle(&mut self, context: &mut EventContext<'_>) {
        self.set_playing(!self.playing);
        context.request_redraw();
        self.play_toggled.emit(&self.playing);
    }

    fn seek_to(&mut self, position: Duration, context: &mut EventContext<'_>) {
        let position = if self.duration.is_zero() {
            position
        } else {
            position.min(self.duration)
        };
        if position == self.position {
            return;
        }
        self.set_position(position);
        context.request_redraw();
        self.seek_requested.emit(&position);
    }

    fn seek_by(&mut self, seconds: f32, context: &mut EventContext<'_>) {
        let position = self.position.as_secs_f32() + seconds;
        let position = if self.duration.is_zero() {
            position.max(0.0)
        } else {
            position.clamp(0.0, self.duration.as_secs_f32())
        };
        self.seek_to(Duration::from_secs_f32(position), context);
    }

    fn seek_from_pointer(&mut self, x: f32, context: &mut EventContext<'_>) {
        let track = progress_track(Rect::new(Point::ZERO, self.viewport));
        let ratio = if track.size.width() > 0.0 {
            ((x - track.origin.x) / track.size.width()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let position = self.duration.mul_f32(ratio);
        self.seek_to(position, context);
    }

    fn paint_play_icon(&self, context: &mut PaintContext<'_>, button: Rect, color: Color) {
        let center = Point::new(
            button.origin.x + button.size.width() / 2.0,
            button.origin.y + button.size.height() / 2.0,
        );
        let height = button.size.height() * 0.58;
        let width = height * 0.82;
        if self.playing {
            let bar = (width * 0.28).max(2.0);
            let gap = (width * 0.18).max(2.0);
            let y = center.y - height / 2.0;
            for offset in [-1.0_f32, 1.0] {
                let x = center.x + offset * (bar + gap) / 2.0 - bar / 2.0;
                context.fill_rounded_rect(
                    Rect::new(
                        Point::new(x, y),
                        Size::new(bar, height).expect("play icon bar is finite"),
                    ),
                    bar / 2.0,
                    color,
                );
            }
        } else {
            let left = center.x - width / 2.0;
            let top = center.y - height / 2.0;
            let row_height = height / 12.0;
            let mut row = 0.0_f32;
            while row < 12.0 {
                let fraction = (row + 0.5) / 12.0;
                let row_width = width * (1.0 - (2.0 * fraction - 1.0).abs()).max(0.0);
                context.fill_rect(
                    Rect::new(
                        Point::new(left, top + row * row_height),
                        Size::new(row_width, row_height).expect("play icon row is finite"),
                    ),
                    color,
                );
                row += 1.0;
            }
        }
    }
}

impl Default for Video {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Video {
    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("video")
    }

    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let max = constraints.max();
        let intrinsic = self.frame.as_ref().map_or(Size::ZERO, image_size);
        let width = if max.width().is_finite() {
            max.width()
        } else if intrinsic.width() > 0.0 {
            intrinsic.width()
        } else {
            DEFAULT_WIDTH
        };
        let height = if max.height().is_finite() {
            max.height()
        } else if intrinsic.height() > 0.0 {
            intrinsic.height()
        } else {
            DEFAULT_HEIGHT
        };
        self.viewport =
            constraints.constrain(Size::new(width, height).expect("video sizes are non-negative"));
        LayoutResult::new(self.viewport)
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &TssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        let background = self
            .background
            .or(style.background)
            .unwrap_or_else(|| environment.theme().background());
        context.fill_rect(bounds, background);
        if let Some(frame) = &self.frame
            && frame.width() > 0
            && frame.height() > 0
            && !bounds.is_empty()
        {
            let destination = fitted_rect(
                bounds,
                image_size(frame),
                self.fit,
                crate::Alignment::Center,
                crate::Alignment::Center,
            );
            context.with_clip(bounds, |context| {
                context.draw_image(frame.clone(), destination);
            });
        }
        if !self.controls || bounds.size.height() < CONTROL_HEIGHT || bounds.is_empty() {
            return;
        }

        let bar = control_bar(bounds);
        context.fill_rect(bar, Color::rgba8(0, 0, 0, 150));
        let foreground = style.foreground.unwrap_or(Color::WHITE);
        let accent = style.accent_or(environment.theme().accent());
        let button = play_button(bounds);
        self.paint_play_icon(context, button, foreground);

        let track = progress_track(bounds);
        context.fill_rounded_rect(
            track,
            PROGRESS_HEIGHT / 2.0,
            Color::rgba8(255, 255, 255, 70),
        );
        let ratio = if self.duration > Duration::ZERO {
            (self.position.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let filled_width = track.size.width() * ratio;
        if filled_width > 0.0 {
            context.fill_rounded_rect(
                Rect::new(
                    track.origin,
                    Size::new(filled_width, track.size.height())
                        .expect("filled progress width is finite"),
                ),
                PROGRESS_HEIGHT / 2.0,
                accent,
            );
        }
        let thumb = Rect::new(
            Point::new(
                track.origin.x + filled_width - PROGRESS_HEIGHT,
                track.origin.y - PROGRESS_HEIGHT,
            ),
            Size::new(PROGRESS_HEIGHT * 3.0, PROGRESS_HEIGHT * 3.0)
                .expect("progress thumb size is finite"),
        );
        context.fill_rounded_rect(thumb, PROGRESS_HEIGHT * 1.5, accent);

        let mut font = environment.font_for(style);
        if style.font_size.is_none() {
            font.set_pixel_size(12.0);
        }
        let time = format!(
            "{} / {}",
            format_time(self.position),
            format_time(self.duration)
        );
        let layout = environment.text_shaper().layout(
            &time,
            &TextStyle::new_with_font(font, foreground),
            None,
        );
        context.draw_text(
            layout.clone(),
            Point::new(
                bar.origin.x + bar.size.width() - CONTROL_PADDING - layout.size().width(),
                bar.origin.y + (bar.size.height() - layout.size().height()) / 2.0,
            ),
        );
    }

    fn advance_animation(&mut self, delta: Duration, settle: bool) -> bool {
        let mut changed = self.poll_decoder();
        if settle {
            if !self.playing {
                return changed;
            }
            self.position = if self.duration.is_zero() {
                self.position
            } else {
                self.duration
            };
            self.set_playing(false);
            return true;
        }
        if !self.playing {
            return changed;
        }
        self.position = self.position.saturating_add(delta);
        if self.decoder.is_some() {
            // The decoder reports end-of-stream itself; wall-clock overshoot is
            // harmless because paint clamps the progress ratio.
            return true;
        }
        if self.duration.is_zero() {
            return changed;
        }
        if self.position >= self.duration {
            if self.looping {
                self.position = duration_remainder(self.position, self.duration);
            } else {
                self.position = self.duration;
                self.playing = false;
            }
        }
        changed = true;
        changed
    }

    fn is_animating(&self) -> bool {
        if let Some(decoder) = &self.decoder {
            return self.playing || decoder.is_busy() || decoder.has_pending_frame();
        }
        self.playing && self.duration > Duration::ZERO && self.position < self.duration
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                context.request_focus();
                let bounds = Rect::new(Point::ZERO, self.viewport);
                if self.controls && progress_track(bounds).contains(pointer.position) {
                    self.seeking = Some(pointer.pointer_id);
                    context.capture_pointer(pointer.pointer_id);
                    self.seek_from_pointer(pointer.position.x, context);
                } else {
                    self.toggle(context);
                }
                EventStatus::Handled
            }
            InputEvent::PointerMove(pointer) if self.seeking == Some(pointer.pointer_id) => {
                self.seek_from_pointer(pointer.position.x, context);
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) if self.seeking == Some(pointer.pointer_id) => {
                self.seeking = None;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::PointerCancel(pointer) if self.seeking == Some(pointer.pointer_id) => {
                self.seeking = None;
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) => match &key.key {
                Key::Named(NamedKey::Space) => {
                    self.toggle(context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.seek_by(-SEEK_STEP_SECONDS, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.seek_by(SEEK_STEP_SECONDS, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::Home) => {
                    self.seek_to(Duration::ZERO, context);
                    EventStatus::Handled
                }
                Key::Named(NamedKey::End) => {
                    self.seek_to(self.duration, context);
                    EventStatus::Handled
                }
                _ => EventStatus::Ignored,
            },
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let accessibility = Accessibility::new(AccessibilityRole::Image);
        match &self.label {
            Some(label) => accessibility.with_label(label.clone()),
            None => accessibility,
        }
    }
}

fn control_bar(bounds: Rect) -> Rect {
    let height = CONTROL_HEIGHT.min(bounds.size.height());
    Rect::new(
        Point::new(
            bounds.origin.x,
            bounds.origin.y + bounds.size.height() - height,
        ),
        Size::new(bounds.size.width(), height).expect("control bar size is finite"),
    )
}

fn play_button(bounds: Rect) -> Rect {
    let bar = control_bar(bounds);
    Rect::new(
        Point::new(
            bar.origin.x + CONTROL_PADDING,
            bar.origin.y + (bar.size.height() - PLAY_BUTTON) / 2.0,
        ),
        Size::new(PLAY_BUTTON, PLAY_BUTTON).expect("play button size is finite"),
    )
}

fn progress_track(bounds: Rect) -> Rect {
    let bar = control_bar(bounds);
    let left = bar.origin.x + CONTROL_PADDING + PLAY_BUTTON + CONTROL_PADDING * 0.8;
    let right = bar.origin.x + bar.size.width() - CONTROL_PADDING - TIME_RESERVE;
    Rect::new(
        Point::new(
            left,
            bar.origin.y + (bar.size.height() - PROGRESS_HEIGHT) / 2.0,
        ),
        Size::new((right - left).max(0.0), PROGRESS_HEIGHT).expect("progress track size is finite"),
    )
}

fn format_time(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use torn_core::{Modifiers, PointerButtons, PointerEvent};
    use torn_render::{DisplayCommand, DisplayList};
    use torn_ui::UiRuntime;

    use super::*;

    fn frame() -> RasterImage {
        RasterImage::new(2, 2, vec![255; 16]).expect("valid RGBA image")
    }

    fn pointer(position: Point, button: Option<PointerButton>) -> PointerEvent {
        PointerEvent {
            pointer_id: PointerId(1),
            position,
            button,
            buttons: if button.is_some() {
                PointerButtons::PRIMARY
            } else {
                PointerButtons::NONE
            },
            modifiers: Modifiers::NONE,
        }
    }

    fn viewport() -> Constraints {
        Constraints::tight(Size::new(320.0, 180.0).expect("valid viewport"))
            .expect("valid constraints")
    }

    #[test]
    fn clicking_toggles_playback_and_emits_a_signal() {
        let video = Video::with_frame(frame());
        let observed = Rc::new(Cell::new(false));
        let _subscription = video.play_toggled().subscribe({
            let observed = Rc::clone(&observed);
            move |playing| observed.set(*playing)
        });
        let mut runtime = UiRuntime::new(video);
        runtime.layout(viewport()).expect("video layout succeeds");

        let down = InputEvent::PointerDown(pointer(
            Point::new(100.0, 100.0),
            Some(PointerButton::Primary),
        ));
        assert_eq!(runtime.dispatch_event(&down), EventStatus::Handled);
        assert!(observed.get());
    }

    #[test]
    fn clicking_the_progress_track_seeks() {
        let video = Video::new().with_duration(Duration::from_secs(100));
        let observed = Rc::new(RefCell::new(Duration::ZERO));
        let _subscription = video.seek_requested().subscribe({
            let observed = Rc::clone(&observed);
            move |position| *observed.borrow_mut() = *position
        });
        let mut runtime = UiRuntime::new(video);
        runtime.layout(viewport()).expect("video layout succeeds");

        let bounds = Rect::new(Point::ZERO, Size::new(320.0, 180.0).expect("valid size"));
        let track = progress_track(bounds);
        let down = InputEvent::PointerDown(pointer(
            Point::new(track.origin.x + track.size.width() / 2.0, track.origin.y),
            Some(PointerButton::Primary),
        ));
        assert_eq!(runtime.dispatch_event(&down), EventStatus::Handled);
        let seconds = observed.borrow().as_secs_f32();
        assert!((seconds - 50.0).abs() < 1.0);
    }

    #[test]
    fn playback_advances_and_stops_at_the_end() {
        let mut video = Video::new()
            .with_duration(Duration::from_secs(1))
            .with_playing(true);
        assert!(video.advance_animation(Duration::from_millis(400), false));
        assert_eq!(video.position(), Duration::from_millis(400));
        assert!(video.advance_animation(Duration::from_millis(900), false));
        assert_eq!(video.position(), Duration::from_secs(1));
        assert!(!video.is_playing());
        assert!(!video.is_animating());
    }

    #[test]
    fn looping_playback_wraps_the_position() {
        let mut video = Video::new()
            .with_duration(Duration::from_secs(1))
            .with_looping(true)
            .with_playing(true);
        video.advance_animation(Duration::from_millis(1250), false);
        assert_eq!(video.position(), Duration::from_millis(250));
        assert!(video.is_animating());
    }

    #[test]
    fn paints_the_frame_and_controls() {
        let mut runtime =
            UiRuntime::new(Video::with_frame(frame()).with_duration(Duration::from_secs(90)));
        let mut list = DisplayList::new();
        runtime.layout(viewport()).expect("video layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("video paint succeeds");

        assert!(
            list.commands()
                .iter()
                .any(|command| matches!(command, DisplayCommand::DrawImage { .. }))
        );
        assert!(
            list.commands()
                .iter()
                .any(|command| matches!(command, DisplayCommand::DrawText { .. }))
        );
    }

    #[test]
    fn open_reports_a_missing_ffprobe() {
        let config = FfmpegConfig::new()
            .with_ffmpeg("torn-missing-ffmpeg")
            .with_ffprobe("torn-missing-ffprobe");
        let Err(error) = Video::open_with("clip.mp4", config) else {
            panic!("opening without ffprobe must fail");
        };
        assert!(matches!(error, VideoError::FfprobeNotFound { .. }));
    }

    #[test]
    fn playing_from_the_end_restarts_at_zero() {
        let mut video = Video::new()
            .with_duration(Duration::from_secs(1))
            .with_position(Duration::from_secs(1));
        video.set_playing(true);
        assert_eq!(video.position(), Duration::ZERO);
        assert!(video.is_playing());
    }

    #[test]
    fn formats_times_with_hours_when_needed() {
        assert_eq!(format_time(Duration::from_secs(5)), "0:05");
        assert_eq!(format_time(Duration::from_secs(65)), "1:05");
        assert_eq!(format_time(Duration::from_secs(3665)), "1:01:05");
    }
}
