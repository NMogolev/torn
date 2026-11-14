//! Standard layout and control widgets for Drawen.
//!
//! The crate provides layout primitives (`Row`, `Column`, `Stack`, `Padding`,
//! `SizedBox`, `Align`), input controls, text and media views, navigation
//! chrome, data-driven virtual lists and tables, docking, and semantic status
//! widgets. Every widget implements [`Widget`](drawen_ui::Widget) and resolves
//! its visual values from the runtime theme or a DSS stylesheet.
//!
//! Interactive widgets expose typed signals. For example, [`Button::activated`]
//! returns a `Signal<()>` whose subscription fires after a completed click or
//! keyboard activation.
//!
//! The `audio` feature enables `AudioPaleoSession` and links the `cpal` output
//! backend.

mod alert;
mod align;
mod animated_image;
#[cfg(feature = "audio")]
mod audio_session;
mod avatar;
mod badge;
mod box_widget;
mod button;
mod card;
mod checkbox;
mod divider;
mod dock_area;
mod dropdown;
mod expander;
mod image;
mod image_viewer;
mod list_view;
mod media;
mod menu_bar;
mod padding;
mod paleo_session;
mod progress_bar;
mod radio_button;
mod scroll_view;
mod segmented_control;
mod sized_box;
mod skeleton;
mod slider;
mod spinner;
mod splitter;
mod stack;
mod switch;
mod tab_bar;
mod text;
mod text_edit;
mod text_editor;
mod text_field;
mod title_bar;
mod toolbar;
mod video;
mod video_ffmpeg;
mod virtual_list;
mod window_frame;

pub use alert::{Alert, AlertTone};
pub use align::{Align, Alignment};
pub use animated_image::{AnimatedImage, AnimationFrame, GifDecodeError};
#[cfg(feature = "audio")]
pub use audio_session::AudioPaleoSession;
pub use avatar::{Avatar, AvatarStatus};
pub use badge::{Badge, BadgeTone};
pub use box_widget::Box;
pub use button::{Button, ButtonStyle};
pub use card::{Card, CardStyle};
pub use checkbox::Checkbox;
pub use divider::{Divider, DividerAxis};
pub use dock_area::{DockArea, DockAreaError, DockItem};
pub use dropdown::Dropdown;
pub use expander::Expander;
pub use image::{Image, ImageFit};
pub use image_viewer::ImageViewer;
pub use list_view::ListView;
pub use media::{
    MediaClock, MediaError, MediaFrame, MediaFrameScheduler, MediaMetadata, MediaSession,
    MediaState, MediaUpdate, VideoMetadata,
};
pub use menu_bar::MenuBar;
pub use padding::Padding;
pub use paleo_session::PaleoSession;
pub use progress_bar::ProgressBar;
pub use radio_button::{RadioButton, RadioGroup};
pub use scroll_view::{ScrollAxis, ScrollView};
pub use segmented_control::SegmentedControl;
pub use sized_box::SizedBox;
pub use skeleton::Skeleton;
pub use slider::Slider;
pub use spinner::Spinner;
pub use splitter::{SplitAxis, Splitter};
pub use stack::Stack;
pub use switch::Switch;
pub use tab_bar::TabBar;
pub use text::Text;
pub use text_edit::TextEdit;
pub use text_field::TextField;
pub use title_bar::{TitleBar, TitleBarStyle, WindowChromeState, WindowControl};
pub use toolbar::Toolbar;
pub use video::Video;
pub use video_ffmpeg::{FfmpegConfig, VideoError};
pub use virtual_list::{
    Column, SelectionMode, SelectionModel, VirtualDataSource, VirtualList, VirtualTable,
    VirtualTableSource,
};
pub use window_frame::WindowFrame;

/// A single-child surface container.
///
/// This is an alias for [`Box`]. Both names describe the same widget and have
/// the same API.
pub type Container = Box;

/// Compatibility alias for [`TextField`].
pub type TextInput = TextField;

/// Compatibility alias for [`Dropdown`].
pub type ComboBox = Dropdown;
