//! Clipboard contracts shared by editable widgets and platform adapters.

/// A source and sink for plain-text clipboard contents.
///
/// Editable controls use this trait instead of depending on a native windowing
/// backend. Applications pass an adapter backed by the operating system
/// clipboard; headless tests and embedded scenes can use [`MemoryClipboard`].
pub trait Clipboard {
    /// Returns the clipboard's current plain text, if any.
    fn read_text(&mut self) -> Option<String>;

    /// Replaces the clipboard's plain-text contents.
    fn write_text(&mut self, text: &str);
}

/// An in-memory clipboard useful for tests and applications without a native
/// clipboard service.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryClipboard {
    text: String,
}

impl MemoryClipboard {
    /// Creates an empty clipboard.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    /// Returns the clipboard contents.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Clipboard for MemoryClipboard {
    fn read_text(&mut self) -> Option<String> {
        (!self.text.is_empty()).then(|| self.text.clone())
    }

    fn write_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
    }
}

/// A clipboard that discards writes and never produces text.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoopClipboard;

impl Clipboard for NoopClipboard {
    fn read_text(&mut self) -> Option<String> {
        None
    }

    fn write_text(&mut self, _text: &str) {}
}
