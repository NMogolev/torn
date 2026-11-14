//! Policies that translate window visibility into playback intent.

/// What playback should do while the window is not presented.
///
/// The policy applies to [`crate::AppLifecycle::WindowOccluded`] and
/// [`crate::AppLifecycle::WindowMinimized`]. A visible window always continues.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VisibilityPolicy {
    /// Keep decoding, presenting, and playing as if the window were visible.
    ///
    /// This is the right choice for offline rendering or for an application
    /// that keeps its window painted by other means.
    Continue,
    /// Keep audio playing and stop video decoding and presentation.
    ///
    /// This is the default: it is what most desktop media players do when the
    /// user minimizes the window, and it costs almost nothing while hidden.
    /// Sessions that cannot pause video independently are paused as a whole.
    #[default]
    PauseVideoKeepAudio,
    /// Pause audio and video, keeping the position.
    PauseAll,
    /// Stop playback and release the media session.
    ///
    /// The application must reopen the media when the window is restored;
    /// everything else is discarded to free decode state and memory.
    StopAndRelease,
}

impl VisibilityPolicy {
    /// Returns the playback intent this policy produces.
    #[must_use]
    pub const fn action(self) -> PlaybackAction {
        match self {
            Self::Continue => PlaybackAction::Continue,
            Self::PauseVideoKeepAudio => PlaybackAction::PauseVideo,
            Self::PauseAll => PlaybackAction::PauseAll,
            Self::StopAndRelease => PlaybackAction::StopAndRelease,
        }
    }
}

/// What a director asks playback to do.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlaybackAction {
    /// Keep decoding, presenting, and playing.
    #[default]
    Continue,
    /// Stop video decoding and presentation; audio keeps playing.
    ///
    /// A session that cannot pause video independently is paused as a whole by
    /// [`crate::MediaDirector::apply`].
    PauseVideo,
    /// Stop both audio and video and keep the position.
    PauseAll,
    /// Stop playback and release the session.
    StopAndRelease,
}

impl PlaybackAction {
    /// Returns whether video should be painted in this state.
    ///
    /// This is intent, not a capability check: use
    /// `Video::supports_video_pause` before assuming audio can continue
    /// without video.
    #[must_use]
    pub const fn presents_video(self) -> bool {
        matches!(self, Self::Continue)
    }

    /// Returns whether audio should keep playing in this state.
    #[must_use]
    pub const fn plays_audio(self) -> bool {
        matches!(self, Self::Continue | Self::PauseVideo)
    }

    /// Returns whether the application should drop the session.
    #[must_use]
    pub const fn releases_session(self) -> bool {
        matches!(self, Self::StopAndRelease)
    }
}
