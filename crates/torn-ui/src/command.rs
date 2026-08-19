use torn_core::{Key, KeyEvent, Modifiers};

use crate::Signal;

/// A keyboard shortcut consisting of a logical key and required modifiers.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeyChord {
    key: Key,
    modifiers: Modifiers,
}

impl KeyChord {
    /// Creates a shortcut activated by `key` while exactly `modifiers` are held.
    #[must_use]
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Returns whether this shortcut matches a key event.
    #[must_use]
    pub fn matches(&self, event: &KeyEvent) -> bool {
        self.key == event.key && self.modifiers == event.modifiers
    }
}

/// An application-level keyboard command.
///
/// Register it with [`crate::UiRuntime::register_command`]. Commands run before
/// the focused widget receives a matching key-down event.
#[derive(Clone, Debug)]
pub struct KeyboardCommand {
    shortcut: KeyChord,
    activated: Signal<()>,
}

impl KeyboardCommand {
    /// Creates a command for `shortcut`.
    #[must_use]
    pub fn new(shortcut: KeyChord) -> Self {
        Self {
            shortcut,
            activated: Signal::new(),
        }
    }

    /// Returns the shortcut that activates this command.
    #[must_use]
    pub const fn shortcut(&self) -> &KeyChord {
        &self.shortcut
    }

    /// Returns a channel notified when the command is invoked.
    #[must_use]
    pub fn activated(&self) -> Signal<()> {
        self.activated.clone()
    }

    pub(crate) fn activate(&self) {
        self.activated.emit(&());
    }
}
