//! The director: window lifecycle in, playback intent and window actions out.

use drawen_platform::{WindowAction, WindowEvent};
use drawen_widgets::Video;

use crate::lifecycle::AppLifecycle;
use crate::policy::{PlaybackAction, VisibilityPolicy};

/// The effect of one lifecycle transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectorAction {
    lifecycle: AppLifecycle,
    playback: PlaybackAction,
    window: WindowAction,
    redraw: bool,
}

impl DirectorAction {
    /// Returns the lifecycle state after the transition.
    #[must_use]
    pub const fn lifecycle(self) -> AppLifecycle {
        self.lifecycle
    }

    /// Returns what playback should do now.
    #[must_use]
    pub const fn playback(self) -> PlaybackAction {
        self.playback
    }

    /// Returns the window action the adapter should apply.
    #[must_use]
    pub const fn window(self) -> WindowAction {
        self.window
    }

    /// Returns whether the application should request a redraw.
    ///
    /// This is `true` when a window that was not presented becomes visible
    /// again, so a paused presenter repaints without waiting for input.
    #[must_use]
    pub const fn redraw(self) -> bool {
        self.redraw
    }
}

/// The presentation flags reported for the tracked window.
///
/// They are kept apart from the derived [`AppLifecycle`] so that an occluded
/// window that is also minimized restores to "occluded", not "visible".
#[derive(Clone, Copy, Debug)]
struct Presentation {
    visible: bool,
    minimized: bool,
    occluded: bool,
}

impl Presentation {
    /// A window that is on screen with no other state set.
    const VISIBLE: Self = Self {
        visible: true,
        minimized: false,
        occluded: false,
    };

    /// No window is presented.
    const CLOSED: Self = Self {
        visible: false,
        minimized: false,
        occluded: false,
    };

    const fn lifecycle(self) -> AppLifecycle {
        if self.minimized {
            AppLifecycle::WindowMinimized
        } else if self.occluded || !self.visible {
            AppLifecycle::WindowOccluded
        } else {
            AppLifecycle::WindowVisible
        }
    }
}

/// Owns lifecycle policy for an application that plays media.
///
/// The director is deliberately not a widget and does not own a decoder. It
/// converts [`WindowEvent`] lifecycle states into [`DirectorAction`]s and
/// applies them to a [`Video`] surface. A widget never decides that a minimized
/// window means silence; that decision belongs here or in the application.
///
/// `MediaDirector::new` assumes one window. Applications that create more call
/// [`Self::open_window`] and [`Self::close_window`]; the director counts them
/// only to detect the last close. Visibility flags describe the presented
/// window, so a multi-window application that needs per-window policy should
/// keep one director per presented window.
#[derive(Debug)]
pub struct MediaDirector {
    policy: VisibilityPolicy,
    lifecycle: AppLifecycle,
    background_playback: bool,
    windows: usize,
    presentation: Presentation,
    paused_by_policy: bool,
}

impl Default for MediaDirector {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaDirector {
    /// Creates a director for one visible window with the default policy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            policy: VisibilityPolicy::default(),
            lifecycle: AppLifecycle::WindowVisible,
            background_playback: false,
            windows: 1,
            presentation: Presentation::VISIBLE,
            paused_by_policy: false,
        }
    }

    /// Selects what happens while the window is minimized or occluded.
    #[must_use]
    pub fn with_policy(mut self, policy: VisibilityPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Allows playback to continue after the last window closes.
    ///
    /// By default the last window closing exits the application. Background
    /// playback is an explicit product decision, so a soundtrack never keeps a
    /// process alive by accident.
    #[must_use]
    pub fn with_background_playback(mut self, background: bool) -> Self {
        self.background_playback = background;
        self
    }

    /// Returns the visibility policy.
    #[must_use]
    pub const fn policy(&self) -> VisibilityPolicy {
        self.policy
    }

    /// Returns whether background playback is allowed after the last close.
    #[must_use]
    pub const fn background_playback(&self) -> bool {
        self.background_playback
    }

    /// Returns the current lifecycle state.
    #[must_use]
    pub const fn lifecycle(&self) -> AppLifecycle {
        self.lifecycle
    }

    /// Returns what playback should be doing in the current state.
    #[must_use]
    pub const fn playback(&self) -> PlaybackAction {
        self.playback_for(self.lifecycle)
    }

    /// Returns how many windows the director believes are open.
    #[must_use]
    pub const fn windows(&self) -> usize {
        self.windows
    }

    /// Returns the action for the current state without changing anything.
    ///
    /// This is useful right after startup, when an application wants to apply
    /// the initial policy before the first window event arrives.
    #[must_use]
    pub fn current_action(&self) -> DirectorAction {
        self.derive_action(self.lifecycle)
    }

    /// Changes the visibility policy and returns the resulting action.
    pub fn set_policy(&mut self, policy: VisibilityPolicy) -> DirectorAction {
        self.policy = policy;
        self.derive_action(self.lifecycle)
    }

    /// Handles a platform event.
    ///
    /// Returns `None` for events the director does not interpret: input,
    /// resizes, redraw requests, and the like. `CloseRequested` is treated as
    /// closing one window, so a single-window application can forward every
    /// event without extra bookkeeping.
    pub fn window_event(&mut self, event: &WindowEvent) -> Option<DirectorAction> {
        match event {
            WindowEvent::CloseRequested => Some(self.close_window()),
            _ if self.lifecycle.is_terminal() => None,
            WindowEvent::Visible(visible) => {
                self.presentation.visible = *visible;
                Some(self.refresh())
            }
            WindowEvent::Minimized(minimized) => {
                self.presentation.minimized = *minimized;
                Some(self.refresh())
            }
            WindowEvent::Occluded(occluded) => {
                self.presentation.occluded = *occluded;
                Some(self.refresh())
            }
            _ => None,
        }
    }

    /// Records that a window was created.
    ///
    /// The first window makes the application presentable again; later windows
    /// only change the count.
    pub fn open_window(&mut self) -> DirectorAction {
        self.windows = self.windows.saturating_add(1);
        if self.windows == 1 {
            self.presentation = Presentation::VISIBLE;
        }
        self.refresh()
    }

    /// Records that a window was closed.
    ///
    /// Closing the last window produces [`AppLifecycle::LastWindowClosed`] and,
    /// unless background playback is allowed, [`WindowAction::Exit`].
    pub fn close_window(&mut self) -> DirectorAction {
        self.windows = self.windows.saturating_sub(1);
        if self.windows == 0 {
            self.presentation = Presentation::CLOSED;
        }
        self.refresh()
    }

    /// Records that the application is shutting down.
    ///
    /// This is the only terminal transition: later window events are ignored
    /// because the event loop is leaving.
    pub fn exiting(&mut self) -> DirectorAction {
        let previous = self.lifecycle;
        self.lifecycle = AppLifecycle::ApplicationExiting;
        self.derive_action(previous)
    }

    /// Applies an action to a video surface.
    ///
    /// The director tracks whether it paused playback itself, so a policy pause
    /// resumes on restore while a user pause stays paused. `PauseVideo` on a
    /// session without an independent audio path degrades to `PauseAll`, because
    /// otherwise video would keep decoding into a hidden window or the clock
    /// would run on with nothing audible.
    pub fn apply(&mut self, action: DirectorAction, video: &mut Video) {
        match action.playback {
            PlaybackAction::Continue => {
                video.set_video_enabled(true);
                if self.paused_by_policy {
                    self.paused_by_policy = false;
                    video.set_playing(true);
                }
            }
            PlaybackAction::PauseVideo => {
                if video.supports_video_pause() {
                    video.set_video_enabled(false);
                } else {
                    self.pause_all(video);
                }
            }
            PlaybackAction::PauseAll => self.pause_all(video),
            PlaybackAction::StopAndRelease => {
                self.paused_by_policy = false;
                video.set_playing(false);
                video.release_session();
            }
        }
    }

    fn pause_all(&mut self, video: &mut Video) {
        if video.is_playing() {
            self.paused_by_policy = true;
        }
        video.set_playing(false);
    }

    fn refresh(&mut self) -> DirectorAction {
        let previous = self.lifecycle;
        if self.windows == 0 {
            if !previous.is_terminal() {
                self.lifecycle = AppLifecycle::LastWindowClosed;
            }
        } else {
            self.lifecycle = self.presentation.lifecycle();
        }
        self.derive_action(previous)
    }

    fn derive_action(&self, previous: AppLifecycle) -> DirectorAction {
        DirectorAction {
            lifecycle: self.lifecycle,
            playback: self.playback_for(self.lifecycle),
            window: self.window_action(),
            redraw: self.lifecycle == AppLifecycle::WindowVisible
                && previous != AppLifecycle::WindowVisible,
        }
    }

    const fn playback_for(&self, lifecycle: AppLifecycle) -> PlaybackAction {
        match lifecycle {
            AppLifecycle::WindowVisible => PlaybackAction::Continue,
            AppLifecycle::ApplicationExiting => PlaybackAction::StopAndRelease,
            AppLifecycle::LastWindowClosed => {
                if self.background_playback {
                    self.policy.action()
                } else {
                    PlaybackAction::StopAndRelease
                }
            }
            AppLifecycle::WindowOccluded | AppLifecycle::WindowMinimized => self.policy.action(),
        }
    }

    const fn window_action(&self) -> WindowAction {
        match self.lifecycle {
            AppLifecycle::ApplicationExiting => WindowAction::Exit,
            AppLifecycle::LastWindowClosed if !self.background_playback => WindowAction::Exit,
            _ => WindowAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppLifecycle, MediaDirector, PlaybackAction, VisibilityPolicy, WindowAction};
    use drawen_platform::WindowEvent;

    #[test]
    fn a_new_director_is_visible_and_continues() {
        let director = MediaDirector::new();
        assert_eq!(director.lifecycle(), AppLifecycle::WindowVisible);
        assert_eq!(director.playback(), PlaybackAction::Continue);
        assert_eq!(director.windows(), 1);
        let action = director.current_action();
        assert_eq!(action.playback(), PlaybackAction::Continue);
        assert_eq!(action.window(), WindowAction::None);
        assert!(!action.redraw());
    }

    #[test]
    fn minimizing_pauses_video_and_keeps_audio_by_default() {
        let mut director = MediaDirector::new();
        let action = director
            .window_event(&WindowEvent::Minimized(true))
            .expect("minimizing is a lifecycle event");
        assert_eq!(action.lifecycle(), AppLifecycle::WindowMinimized);
        assert_eq!(action.playback(), PlaybackAction::PauseVideo);
        assert_eq!(action.window(), WindowAction::None);
        assert!(action.playback().plays_audio());
        assert!(!action.playback().presents_video());
    }

    #[test]
    fn restoring_requests_a_redraw() {
        let mut director = MediaDirector::new();
        director.window_event(&WindowEvent::Minimized(true));
        let action = director
            .window_event(&WindowEvent::Minimized(false))
            .expect("restoring is a lifecycle event");
        assert_eq!(action.lifecycle(), AppLifecycle::WindowVisible);
        assert_eq!(action.playback(), PlaybackAction::Continue);
        assert!(action.redraw());
    }

    #[test]
    fn minimizing_wins_over_occlusion() {
        let mut director = MediaDirector::new();
        director.window_event(&WindowEvent::Minimized(true));
        director.window_event(&WindowEvent::Occluded(true));
        assert_eq!(director.lifecycle(), AppLifecycle::WindowMinimized);
        director.window_event(&WindowEvent::Minimized(false));
        assert_eq!(director.lifecycle(), AppLifecycle::WindowOccluded);
    }

    #[test]
    fn hiding_maps_to_occlusion() {
        let mut director = MediaDirector::new();
        let action = director
            .window_event(&WindowEvent::Visible(false))
            .expect("visibility is a lifecycle event");
        assert_eq!(action.lifecycle(), AppLifecycle::WindowOccluded);
        assert_eq!(action.playback(), PlaybackAction::PauseVideo);
    }

    #[test]
    fn each_policy_maps_to_its_playback_action() {
        for (policy, expected) in [
            (VisibilityPolicy::Continue, PlaybackAction::Continue),
            (
                VisibilityPolicy::PauseVideoKeepAudio,
                PlaybackAction::PauseVideo,
            ),
            (VisibilityPolicy::PauseAll, PlaybackAction::PauseAll),
            (
                VisibilityPolicy::StopAndRelease,
                PlaybackAction::StopAndRelease,
            ),
        ] {
            let mut director = MediaDirector::new().with_policy(policy);
            let action = director
                .window_event(&WindowEvent::Occluded(true))
                .expect("occlusion is a lifecycle event");
            assert_eq!(action.playback(), expected, "policy {policy:?}");
        }
    }

    #[test]
    fn changing_the_policy_recomputes_the_current_state() {
        let mut director = MediaDirector::new();
        director.window_event(&WindowEvent::Occluded(true));
        let action = director.set_policy(VisibilityPolicy::Continue);
        assert_eq!(action.playback(), PlaybackAction::Continue);
        assert_eq!(director.playback(), PlaybackAction::Continue);
    }

    #[test]
    fn closing_the_last_window_exits() {
        let mut director = MediaDirector::new();
        let action = director
            .window_event(&WindowEvent::CloseRequested)
            .expect("a close request is a lifecycle event");
        assert_eq!(action.lifecycle(), AppLifecycle::LastWindowClosed);
        assert_eq!(action.playback(), PlaybackAction::StopAndRelease);
        assert_eq!(action.window(), WindowAction::Exit);
        assert_eq!(director.windows(), 0);
        assert!(!action.lifecycle().has_window());
    }

    #[test]
    fn closing_one_of_two_windows_keeps_running() {
        let mut director = MediaDirector::new();
        director.open_window();
        assert_eq!(director.windows(), 2);

        let action = director
            .window_event(&WindowEvent::CloseRequested)
            .expect("a close request is a lifecycle event");
        assert_eq!(action.lifecycle(), AppLifecycle::WindowVisible);
        assert_eq!(action.playback(), PlaybackAction::Continue);
        assert_eq!(action.window(), WindowAction::None);

        let action = director.close_window();
        assert_eq!(action.lifecycle(), AppLifecycle::LastWindowClosed);
        assert_eq!(action.window(), WindowAction::Exit);
    }

    #[test]
    fn background_playback_keeps_audio_after_the_last_close() {
        let mut director = MediaDirector::new().with_background_playback(true);
        let action = director.close_window();
        assert_eq!(action.lifecycle(), AppLifecycle::LastWindowClosed);
        assert_eq!(action.playback(), PlaybackAction::PauseVideo);
        assert_eq!(action.window(), WindowAction::None);
        assert!(action.playback().plays_audio());
    }

    #[test]
    fn background_playback_still_honors_a_silent_policy() {
        let mut director = MediaDirector::new()
            .with_background_playback(true)
            .with_policy(VisibilityPolicy::PauseAll);
        let action = director.close_window();
        assert_eq!(action.playback(), PlaybackAction::PauseAll);
        assert_eq!(action.window(), WindowAction::None);
    }

    #[test]
    fn reopening_a_window_makes_the_application_visible_again() {
        let mut director = MediaDirector::new();
        director.close_window();
        let action = director.open_window();
        assert_eq!(action.lifecycle(), AppLifecycle::WindowVisible);
        assert_eq!(action.playback(), PlaybackAction::Continue);
        assert!(action.redraw());
    }

    #[test]
    fn exiting_is_terminal_for_later_events() {
        let mut director = MediaDirector::new();
        let action = director.exiting();
        assert_eq!(action.lifecycle(), AppLifecycle::ApplicationExiting);
        assert_eq!(action.playback(), PlaybackAction::StopAndRelease);
        assert_eq!(action.window(), WindowAction::Exit);
        assert!(action.lifecycle().is_terminal());

        assert!(
            director.window_event(&WindowEvent::Visible(true)).is_none(),
            "a leaving application ignores window state"
        );
        assert_eq!(director.lifecycle(), AppLifecycle::ApplicationExiting);
    }

    #[test]
    fn unrelated_events_are_ignored() {
        let mut director = MediaDirector::new();
        assert!(
            director
                .window_event(&WindowEvent::RedrawRequested)
                .is_none()
        );
        assert!(
            director
                .window_event(&WindowEvent::Maximized(true))
                .is_none()
        );
        assert_eq!(director.lifecycle(), AppLifecycle::WindowVisible);
    }

    #[test]
    fn playback_actions_describe_their_effect() {
        assert!(PlaybackAction::Continue.presents_video());
        assert!(PlaybackAction::Continue.plays_audio());
        assert!(!PlaybackAction::PauseVideo.presents_video());
        assert!(PlaybackAction::PauseVideo.plays_audio());
        assert!(!PlaybackAction::PauseAll.plays_audio());
        assert!(!PlaybackAction::StopAndRelease.plays_audio());
        assert!(PlaybackAction::StopAndRelease.releases_session());
        assert!(!PlaybackAction::PauseAll.releases_session());
    }

    fn playing_video() -> drawen_widgets::Video {
        let mut video = drawen_widgets::Video::new();
        video.set_duration(std::time::Duration::from_secs(4));
        video.set_position(std::time::Duration::from_secs(1));
        video.set_playing(true);
        video
    }

    #[test]
    fn applying_pause_all_resumes_only_what_it_paused() {
        let mut director = MediaDirector::new();
        let mut video = playing_video();

        let paused = director
            .window_event(&WindowEvent::Minimized(true))
            .expect("minimizing is a lifecycle event");
        assert_eq!(paused.playback(), PlaybackAction::PauseVideo);
        director.apply(paused, &mut video);
        // A manual clock has no independent audio path, so the video-only
        // fallback pauses it instead of leaving the clock running.
        assert!(!video.is_playing());

        let restored = director
            .window_event(&WindowEvent::Minimized(false))
            .expect("restoring is a lifecycle event");
        director.apply(restored, &mut video);
        assert!(video.is_playing(), "the policy pause is resumed");
    }

    #[test]
    fn a_user_pause_survives_a_visibility_round_trip() {
        let mut director = MediaDirector::new().with_policy(VisibilityPolicy::PauseAll);
        let mut video = playing_video();
        video.set_playing(false);

        let hidden = director
            .window_event(&WindowEvent::Occluded(true))
            .expect("occlusion is a lifecycle event");
        director.apply(hidden, &mut video);
        let shown = director
            .window_event(&WindowEvent::Occluded(false))
            .expect("restoring is a lifecycle event");
        director.apply(shown, &mut video);

        assert!(!video.is_playing(), "a user pause is not overridden");
    }
}
