//! Optional runtime orchestration for Torn applications that play media.
//!
//! Widgets present and control media; they do not decide what a minimized or
//! occluded window means. That decision is a product policy, so it lives in
//! this separate crate rather than in `torn-widgets`:
//!
//! - [`VisibilityPolicy`] says what should happen while the window is not
//!   presented: continue, pause video but keep audio, pause everything, or stop
//!   and release the session.
//! - [`AppLifecycle`] names the normalized window and application states that
//!   the policy reacts to.
//! - [`MediaDirector`] turns platform [`WindowEvent`]s into [`DirectorAction`]s
//!   and applies them to a [`Video`] surface.
//!
//! The defaults match desktop expectations: audio keeps playing while video
//! stops when a window is minimized or occluded, and closing the last window
//! exits the application. Background playback after the last close must be
//! requested with [`MediaDirector::with_background_playback`].
//!
//! # Wiring
//!
//! ```no_run
//! use torn_director::{MediaDirector, VisibilityPolicy};
//! use torn_platform::{WindowAction, WindowEvent};
//! use torn_widgets::Video;
//!
//! fn handle(
//!     director: &mut MediaDirector,
//!     video: &mut Video,
//!     event: &WindowEvent,
//! ) -> WindowAction {
//!     if let Some(action) = director.window_event(event) {
//!         director.apply(action, video);
//!         return action.window();
//!     }
//!     WindowAction::None
//! }
//!
//! # fn run() {
//! let mut director = MediaDirector::new().with_policy(VisibilityPolicy::PauseVideoKeepAudio);
//! let mut video = Video::new();
//! let action = director.window_event(&WindowEvent::Minimized(true));
//! if let Some(action) = action {
//!     director.apply(action, &mut video);
//! }
//! # }
//! ```
//!
//! [`WindowEvent`]: torn_platform::WindowEvent

#![forbid(unsafe_code)]

mod director;
mod lifecycle;
mod policy;

pub use director::{DirectorAction, MediaDirector};
pub use lifecycle::AppLifecycle;
pub use policy::{PlaybackAction, VisibilityPolicy};
