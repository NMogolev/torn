//! Application lifecycle states that a media policy reacts to.
//!
//! The states are normalized: they describe what the operating system or the
//! user did, not what playback should do about it. [`crate::MediaDirector`]
//! maps each state to a [`crate::PlaybackAction`] with the configured
//! [`crate::VisibilityPolicy`].

/// The lifecycle state of the application's presentation window.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AppLifecycle {
    /// The window is on screen and can present frames.
    #[default]
    WindowVisible,
    /// The window is hidden, fully covered, or otherwise not presented.
    ///
    /// Audio may keep playing; see [`crate::VisibilityPolicy`].
    WindowOccluded,
    /// The user minimized the window.
    ///
    /// This implies occlusion, but it is tracked separately because it is an
    /// explicit request to put the window in the background.
    WindowMinimized,
    /// The last window closed.
    ///
    /// By default this means the application exits. Background playback is an
    /// explicit application policy, enabled with
    /// [`crate::MediaDirector::with_background_playback`].
    LastWindowClosed,
    /// The application is shutting down.
    ApplicationExiting,
}

impl AppLifecycle {
    /// Returns whether the window can present frames in this state.
    #[must_use]
    pub const fn is_presented(self) -> bool {
        matches!(self, Self::WindowVisible)
    }

    /// Returns whether a window is still open in this state.
    #[must_use]
    pub const fn has_window(self) -> bool {
        !matches!(self, Self::LastWindowClosed | Self::ApplicationExiting)
    }

    /// Returns whether the application should stop its event loop.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::ApplicationExiting)
    }
}
