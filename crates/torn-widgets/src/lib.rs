//! Standard layout and control widgets for Torn.

mod align;
mod box_widget;
mod button;
mod checkbox;
mod dock_area;
mod list_view;
mod padding;
mod radio_button;
mod scroll_view;
mod sized_box;
mod splitter;
mod stack;
mod text;
mod text_field;

pub use align::{Align, Alignment};
pub use box_widget::Box;
pub use button::{Button, ButtonStyle};
pub use checkbox::Checkbox;
pub use dock_area::{DockArea, DockAreaError, DockItem};
pub use list_view::ListView;
pub use padding::Padding;
pub use radio_button::{RadioButton, RadioGroup};
pub use scroll_view::{ScrollAxis, ScrollView};
pub use sized_box::SizedBox;
pub use splitter::{SplitAxis, Splitter};
pub use stack::Stack;
pub use text::Text;
pub use text_field::TextField;

/// A single-child surface container.
///
/// This is an alias for [`Box`]. Both names describe the same widget and have
/// the same API.
pub type Container = Box;

/// Compatibility alias for [`TextField`].
pub type TextInput = TextField;
