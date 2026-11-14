use std::path::Path;
use std::time::Duration;

use drawen_core::{
    Color, Constraints, InputEvent, Key, NamedKey, Point, PointerButton, PointerId, Rect, Size,
};
use drawen_render::{PaintContext, RasterImage, TextStyle};
use drawen_ui::{
    Accessibility, AccessibilityRole, DssStyle, EventContext, EventPhase, EventStatus,
    LayoutContext, LayoutResult, Signal, StyleTarget, UiEnvironment, Widget,
};

use crate::ImageFit;
use crate::image::{fitted_rect, image_size};
use crate::media::{
    MediaClock, MediaError, MediaFrame, MediaFrameScheduler, MediaMetadata, MediaSession,
};
use crate::paleo_session::PaleoSession;
use crate::video_ffmpeg::{FfmpegConfig, FfmpegSession, VideoError};

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
/// [`Self::open`] attaches the built-in `FFmpeg` command-line backend as a
/// [`MediaSession`]: the session probes metadata, decodes frames on a worker
/// thread, and owns playback timing, while the widget stays a view and control.
/// A [`MediaFrameScheduler`] presents the frame that matches the current
/// presentation time and drops frames that are already stale.
///
/// Applications that already own a decoder can keep using [`Self::set_frame`]
/// and react to [`Self::play_toggled`] and [`Self::seek_requested`] instead. In
/// that manual mode the widget advances a [`MediaClock`] itself so progress and
/// time labels stay live; externally supplied frames should arrive at the
/// matching rate.
pub struct Video {
    clock: MediaClock,
    session: Option<Box<dyn MediaSession>>,
    scheduler: MediaFrameScheduler,
    looping: bool,
    controls: bool,
    fit: ImageFit,
    background: Option<Color>,
    label: Option<String>,
    viewport: Size,
    seeking: Option<PointerId>,
    session_error: Option<String>,
    play_toggled: Signal<bool>,
    seek_requested: Signal<Duration>,
}

impl Video {
    /// Creates an empty video surface with controls enabled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            clock: MediaClock::new(),
            session: None,
            scheduler: MediaFrameScheduler::new(),
            looping: false,
            controls: true,
            fit: ImageFit::Contain,
            background: None,
            label: None,
            viewport: Size::ZERO,
            seeking: None,
            session_error: None,
            play_toggled: Signal::new(),
            seek_requested: Signal::new(),
        }
    }

    /// Creates a video surface showing `frame`.
    #[must_use]
    pub fn with_frame(frame: RasterImage) -> Self {
        let mut video = Self::new();
        video.set_frame(frame);
        video
    }

    /// Opens `path` with the built-in `FFmpeg` backend.
    ///
    /// This starts decoding immediately so the first frame can be shown while
    /// playback is paused. Playback, seeking, and looping then stay in sync
    /// with the session until the widget is dropped or [`Self::close_decoder`]
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
        let session = FfmpegSession::open(path.as_ref(), config)?;
        let mut video = Self::new();
        video.session = Some(Box::new(session));
        Ok(video)
    }

    /// Opens `path` with the pure-Rust Paleo H.264 backend.
    ///
    /// Unlike [`Self::open`], this needs no external executable. It currently
    /// decodes H.264 only; use [`Self::open`] for containers or codecs that
    /// Paleo cannot handle yet. Decoding happens on the calling thread in
    /// bounded steps, and the first picture is ready when this returns.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] when the file cannot be opened, has no video
    /// track, uses a codec other than H.264, or fails to decode.
    pub fn open_paleo(path: impl AsRef<Path>) -> Result<Self, MediaError> {
        let session = PaleoSession::open(path.as_ref())?;
        Ok(Self::new().with_session(session))
    }

    /// Opens `path` with Paleo audio output and follows video to the audio clock.
    ///
    /// Requires the `audio` feature. Audio is decoded by Paleo and played by
    /// `paleo-audio`; the device position is the master clock. If the file has
    /// no audio track, the session falls back to the wall-clock behavior of
    /// [`Self::open_paleo`]. A video track is shown only when it is H.264.
    ///
    /// # Errors
    ///
    /// Returns [`MediaError`] when the file cannot be opened, the audio codec is
    /// unsupported, no output device is available, or the video stream is
    /// malformed.
    #[cfg(feature = "audio")]
    pub fn open_paleo_audio(path: impl AsRef<Path>) -> Result<Self, MediaError> {
        let session = crate::AudioPaleoSession::open(path.as_ref())?;
        Ok(Self::new().with_session(session))
    }

    /// Attaches an externally created [`MediaSession`].
    ///
    /// This is the escape hatch for custom decoders and for backends that are
    /// not built in, such as a future AV1 or hardware session.
    #[must_use]
    pub fn with_session(mut self, session: impl MediaSession + 'static) -> Self {
        self.session = Some(Box::new(session));
        self
    }

    /// Returns the timeline owned by the attached session or by manual mode.
    fn timeline(&self) -> &MediaClock {
        self.session
            .as_deref()
            .map_or(&self.clock, |session| session.clock())
    }

    /// Returns the metadata reported by the built-in session.
    #[must_use]
    pub fn metadata(&self) -> Option<MediaMetadata> {
        self.session
            .as_deref()
            .and_then(|session| session.metadata().copied())
    }

    /// Returns whether a media session is attached.
    #[must_use]
    pub const fn has_decoder(&self) -> bool {
        self.session.is_some()
    }

    /// Returns the most recent session error, if any.
    ///
    /// Decoding happens on a worker thread, so start-up and read failures are
    /// reported here rather than by [`Self::open`]. A reported error stops
    /// playback immediately.
    #[must_use]
    pub fn decoder_error(&self) -> Option<&str> {
        self.session_error.as_deref()
    }

    /// Stops and releases the built-in session while keeping the last frame.
    pub fn close_decoder(&mut self) {
        if let Some(session) = self.session.take() {
            self.clock = *session.clock();
        }
        self.clock.pause();
        self.session_error = None;
    }

    /// Returns the current frame, if one has been supplied or decoded.
    #[must_use]
    pub fn frame(&self) -> Option<&RasterImage> {
        self.scheduler.current().map(MediaFrame::image)
    }

    /// Returns the declared media duration.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.timeline().duration().unwrap_or(Duration::ZERO)
    }

    /// Returns the current playback position.
    #[must_use]
    pub fn position(&self) -> Duration {
        self.timeline().position()
    }

    /// Returns whether playback is currently running.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.timeline().is_playing()
    }

    /// Returns whether playback reached the end of the media.
    #[must_use]
    pub fn is_ended(&self) -> bool {
        self.timeline().is_ended()
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
    ///
    /// In manual mode the frame stays visible until another frame arrives. With
    /// an attached session the next decoded frame wins.
    pub fn set_frame(&mut self, frame: RasterImage) {
        let position = self.position();
        self.scheduler
            .replace(MediaFrame::new(frame, position, Duration::ZERO));
    }

    /// Clears the displayed frame.
    pub fn clear_frame(&mut self) {
        self.scheduler.clear();
    }

    /// Sets the media duration and clamps the current position to it.
    pub fn set_duration(&mut self, duration: Duration) {
        if let Some(session) = self.session.as_mut() {
            session.clock_mut().set_duration(Some(duration));
        } else {
            self.clock.set_duration(Some(duration));
        }
        self.scheduler.seek(self.position());
    }

    /// Sets the playback position, clamping it to `0..=duration`.
    ///
    /// With an attached session this seeks immediately. Seeking while paused
    /// asks the session for a preview frame at the new position.
    pub fn set_position(&mut self, position: Duration) {
        if let Some(session) = self.session.as_mut() {
            if let Err(error) = session.seek(position) {
                self.record_error(&error);
            }
        } else {
            self.clock.seek(position);
        }
        self.scheduler.seek(self.position());
    }

    /// Updates playback state without emitting [`Self::play_toggled`].
    pub fn set_playing(&mut self, playing: bool) {
        if let Some(session) = self.session.as_mut() {
            if playing {
                if let Err(error) = session.play() {
                    self.record_error(&error);
                }
            } else {
                session.pause();
            }
        } else {
            if playing
                && !self.looping
                && !self.duration().is_zero()
                && self.position() >= self.duration()
            {
                self.clock.seek(Duration::ZERO);
            }
            self.clock.set_playing(playing);
        }
    }

    /// Starts or pauses playback without emitting [`Self::play_toggled`].
    pub fn toggle_playback(&mut self) {
        self.set_playing(!self.is_playing());
    }

    /// Changes the playback rate, clamped by the media clock.
    pub fn set_rate(&mut self, rate: f64) {
        if let Some(session) = self.session.as_mut() {
            session.set_rate(rate);
        } else {
            self.clock.set_rate(rate);
        }
    }

    /// Returns the current playback rate.
    #[must_use]
    pub fn rate(&self) -> f64 {
        self.timeline().rate()
    }

    /// Returns whether the attached session can stop video while audio plays.
    ///
    /// A policy layer should pause the whole session when this is `false`;
    /// [`Self::set_video_enabled`] then has no effect.
    #[must_use]
    pub fn supports_video_pause(&self) -> bool {
        self.session
            .as_deref()
            .is_some_and(MediaSession::supports_video_pause)
    }

    /// Suspends or resumes video decoding and presentation.
    ///
    /// Audio keeps playing when the session supports it, which is how a window
    /// that is minimized or occluded can keep its soundtrack. The last painted
    /// frame stays visible. Resuming decodes from the audio clock and discards
    /// pictures that are already behind the playhead.
    pub fn set_video_enabled(&mut self, enabled: bool) {
        if let Some(session) = self.session.as_mut()
            && let Err(error) = session.set_video_enabled(enabled)
        {
            self.record_error(&error);
        }
    }

    /// Detaches the media session and stops playback.
    ///
    /// The last painted frame and the current position are kept, so the view
    /// does not jump when a policy releases a stopped session.
    pub fn release_session(&mut self) {
        if let Some(session) = self.session.as_deref() {
            self.clock = *session.clock();
        }
        if let Some(mut session) = self.session.take() {
            session.pause();
        }
    }

    fn record_error(&mut self, error: &MediaError) {
        self.session_error = Some(error.message().to_owned());
    }

    /// Advances the session or manual clock and presents the matching frame.
    fn poll(&mut self, delta: Duration) -> bool {
        let mut changed = false;
        if let Some(session) = self.session.as_mut() {
            let update = session.update(delta);
            if let Some(frame) = update.frame {
                changed |= self.scheduler.push(frame);
            }
            if let Some(error) = update.error {
                self.session_error = Some(error.message().to_owned());
                changed = true;
            }
            changed |= update.state_changed;
            let position = session.clock().position();
            changed |= self.scheduler.present(position);
            if session.clock().is_ended() {
                if self.looping {
                    session.clock_mut().seek(Duration::ZERO);
                    if let Err(error) = session.play() {
                        self.session_error = Some(error.message().to_owned());
                    }
                    self.scheduler.seek(Duration::ZERO);
                    changed = true;
                } else {
                    changed = true;
                }
            }
        } else {
            if self.clock.is_playing() {
                changed |= if self.looping {
                    self.clock.advance_looping(delta)
                } else {
                    self.clock.advance(delta)
                };
            }
            changed |= self.scheduler.present(self.clock.position());
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
        self.set_playing(!self.is_playing());
        context.request_redraw();
        let playing = self.is_playing();
        self.play_toggled.emit(&playing);
    }

    fn seek_to(&mut self, position: Duration, context: &mut EventContext<'_>) {
        let duration = self.duration();
        let position = if duration.is_zero() {
            position
        } else {
            position.min(duration)
        };
        if position == self.position() {
            return;
        }
        self.set_position(position);
        context.request_redraw();
        self.seek_requested.emit(&position);
    }

    fn seek_by(&mut self, seconds: f32, context: &mut EventContext<'_>) {
        let position = self.position().as_secs_f32() + seconds;
        let duration = self.duration();
        let position = if duration.is_zero() {
            position.max(0.0)
        } else {
            position.clamp(0.0, duration.as_secs_f32())
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
        let position = self.duration().mul_f32(ratio);
        self.seek_to(position, context);
    }

    fn paint_play_icon(&self, context: &mut PaintContext<'_>, button: Rect, color: Color) {
        let center = Point::new(
            button.origin.x + button.size.width() / 2.0,
            button.origin.y + button.size.height() / 2.0,
        );
        let height = button.size.height() * 0.58;
        let width = height * 0.82;
        if self.is_playing() {
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
        let intrinsic = self.frame().map_or(Size::ZERO, image_size);
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
        self.paint_styled(context, environment, bounds, &DssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &DssStyle,
    ) {
        let background = self
            .background
            .or(style.background)
            .unwrap_or_else(|| environment.theme().background());
        context.fill_rect(bounds, background);
        if let Some(frame) = self.scheduler.current() {
            let image = frame.image();
            if image.width() > 0 && image.height() > 0 && !bounds.is_empty() {
                let destination = fitted_rect(
                    bounds,
                    image_size(image),
                    self.fit,
                    crate::Alignment::Center,
                    crate::Alignment::Center,
                );
                let shared = frame.shared_image().clone();
                context.with_clip(bounds, |context| {
                    context.draw_image(shared, destination);
                });
            }
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
        let ratio = if self.duration() > Duration::ZERO {
            (self.position().as_secs_f32() / self.duration().as_secs_f32()).clamp(0.0, 1.0)
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
            format_time(self.position()),
            format_time(self.duration())
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
        let mut changed = self.poll(if settle { Duration::ZERO } else { delta });
        if settle && self.is_playing() {
            self.set_playing(false);
            changed = true;
        }
        changed
    }

    fn is_animating(&self) -> bool {
        if let Some(session) = self.session.as_deref() {
            return session.clock().is_playing() || session.is_active();
        }
        let duration = self.clock.duration().unwrap_or(Duration::ZERO);
        self.clock.is_playing() && !duration.is_zero() && self.clock.position() < duration
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
                    self.seek_to(self.duration(), context);
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

    use drawen_core::{Modifiers, PointerButtons, PointerEvent};
    use drawen_render::{DisplayCommand, DisplayList};
    use drawen_ui::UiRuntime;

    use super::*;
    use crate::media::MediaUpdate;

    fn frame() -> RasterImage {
        RasterImage::new(2, 2, vec![255; 16]).expect("valid RGBA image")
    }

    /// A session with no decoder used to verify that the view only forwards
    /// transport commands and presents frames scheduled by the contract.
    struct MockSession {
        clock: MediaClock,
        metadata: MediaMetadata,
        pending: Vec<MediaFrame>,
        video_pausable: bool,
        video_enabled: bool,
    }

    impl MockSession {
        fn new() -> Self {
            let mut clock = MediaClock::new();
            clock.set_duration(Some(Duration::from_secs(2)));
            clock.seek(Duration::ZERO);
            Self {
                clock,
                metadata: MediaMetadata::new(4, 2, Duration::from_secs(2)),
                pending: Vec::new(),
                video_pausable: false,
                video_enabled: true,
            }
        }
    }

    impl MediaSession for MockSession {
        fn metadata(&self) -> Option<&MediaMetadata> {
            Some(&self.metadata)
        }

        fn clock(&self) -> &MediaClock {
            &self.clock
        }

        fn clock_mut(&mut self) -> &mut MediaClock {
            &mut self.clock
        }

        fn play(&mut self) -> Result<(), MediaError> {
            self.clock.play();
            Ok(())
        }

        fn pause(&mut self) {
            self.clock.pause();
        }

        fn seek(&mut self, position: Duration) -> Result<(), MediaError> {
            self.clock.seek(position);
            Ok(())
        }

        fn set_rate(&mut self, rate: f64) {
            self.clock.set_rate(rate);
        }

        fn supports_video_pause(&self) -> bool {
            self.video_pausable
        }

        fn set_video_enabled(&mut self, enabled: bool) -> Result<bool, MediaError> {
            if !self.video_pausable || self.video_enabled == enabled {
                return Ok(false);
            }
            self.video_enabled = enabled;
            Ok(true)
        }

        fn update(&mut self, delta: Duration) -> MediaUpdate {
            let state_changed = self.clock.advance(delta);
            let position = self.clock.position();
            let index = self
                .pending
                .iter()
                .position(|frame| frame.pts() <= position);
            let frame = index.map(|index| self.pending.remove(index));
            MediaUpdate {
                frame,
                error: None,
                state_changed,
            }
        }

        fn is_active(&self) -> bool {
            !self.pending.is_empty()
        }
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
            .with_ffmpeg("drawen-missing-ffmpeg")
            .with_ffprobe("drawen-missing-ffprobe");
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
    fn transport_commands_are_forwarded_to_the_session() {
        let mut video = Video::new();
        video.session = Some(Box::new(MockSession::new()));
        assert_eq!(video.metadata().map(|metadata| metadata.width()), Some(4));

        video.set_playing(true);
        assert!(video.is_playing());
        video.set_position(Duration::from_secs(1));
        assert_eq!(video.position(), Duration::from_secs(1));
        video.set_rate(2.0);
        assert!((video.rate() - 2.0).abs() < f64::EPSILON);
        video.set_playing(false);
        assert!(!video.is_playing());

        video.close_decoder();
        assert!(!video.has_decoder());
        assert_eq!(video.position(), Duration::from_secs(1));
        assert_eq!(video.duration(), Duration::from_secs(2));
    }

    #[test]
    fn session_frames_reach_the_view_at_their_presentation_time() {
        let mut video = Video::new();
        let mut session = MockSession::new();
        session.pending.push(MediaFrame::new(
            RasterImage::new(3, 3, vec![255; 36]).expect("valid RGBA image"),
            Duration::ZERO,
            Duration::from_millis(100),
        ));
        video.session = Some(Box::new(session));

        assert!(video.advance_animation(Duration::ZERO, false));
        assert_eq!(video.frame().map(RasterImage::width), Some(3));
        assert!(!video.is_animating());
    }

    #[test]
    fn video_pause_is_forwarded_and_release_keeps_the_position() {
        let mut session = MockSession::new();
        session.video_pausable = true;
        session.clock.seek(Duration::from_millis(500));
        let mut video = Video::new();
        video.session = Some(Box::new(session));

        assert!(video.supports_video_pause());
        video.set_video_enabled(false);
        assert_eq!(video.decoder_error(), None);
        video.set_video_enabled(true);
        assert_eq!(video.decoder_error(), None);

        video.release_session();
        assert!(!video.supports_video_pause());
        assert!(!video.has_decoder());
        assert_eq!(video.position(), Duration::from_millis(500));
    }

    #[test]
    fn video_pause_is_ignored_without_session_support() {
        let mut video = Video::new();
        video.session = Some(Box::new(MockSession::new()));

        assert!(!video.supports_video_pause());
        video.set_video_enabled(false);
        assert_eq!(video.decoder_error(), None);
        assert!(video.has_decoder());
    }

    #[test]
    fn formats_times_with_hours_when_needed() {
        assert_eq!(format_time(Duration::from_secs(5)), "0:05");
        assert_eq!(format_time(Duration::from_secs(65)), "1:05");
        assert_eq!(format_time(Duration::from_secs(3665)), "1:01:05");
    }
}
