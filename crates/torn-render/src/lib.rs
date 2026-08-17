//! Backend-neutral display-list and text contracts for Torn.

mod display_list;
mod paint;
mod text;

pub use display_list::{DisplayCommand, DisplayList};
pub use paint::PaintContext;
pub use text::{TextLayout, TextShaper, TextStyle};
