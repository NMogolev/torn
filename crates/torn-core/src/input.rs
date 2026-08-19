use std::collections::BTreeSet;

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

use crate::{Point, WidgetId};

/// A stable identifier for one pointing-device contact.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PointerId(pub u64);

/// A pointer button.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerButton {
    /// The primary pointer button.
    Primary,
    /// The auxiliary pointer button.
    Auxiliary,
    /// The secondary pointer button.
    Secondary,
    /// A platform-specific additional button.
    Other(u16),
}

/// The set of pointer buttons currently pressed.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PointerButtons {
    standard: u8,
    other: BTreeSet<u16>,
}

impl PointerButtons {
    /// No pressed buttons.
    pub const NONE: Self = Self {
        standard: 0,
        other: BTreeSet::new(),
    };
    /// The primary button.
    pub const PRIMARY: Self = Self {
        standard: 1 << 0,
        other: BTreeSet::new(),
    };
    /// The auxiliary button.
    pub const AUXILIARY: Self = Self {
        standard: 1 << 1,
        other: BTreeSet::new(),
    };
    /// The secondary button.
    pub const SECONDARY: Self = Self {
        standard: 1 << 2,
        other: BTreeSet::new(),
    };

    /// Returns a set containing `button`.
    #[must_use]
    pub fn from_button(button: PointerButton) -> Self {
        let mut result = Self::NONE;
        result.insert(button);
        result
    }

    /// Returns the raw bit representation of the standard buttons.
    ///
    /// Platform-specific additional buttons are available through
    /// [`Self::contains_button`].
    #[must_use]
    pub fn bits(&self) -> u16 {
        u16::from(self.standard)
    }

    /// Returns whether no buttons are pressed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.standard == 0 && self.other.is_empty()
    }

    /// Returns whether this set contains all `other` buttons.
    #[must_use]
    pub fn contains(&self, other: &Self) -> bool {
        (self.standard & other.standard) == other.standard
            && other.other.iter().all(|button| self.other.contains(button))
    }

    /// Returns whether `button` is pressed.
    #[must_use]
    pub fn contains_button(&self, button: PointerButton) -> bool {
        match button {
            PointerButton::Primary => self.standard & Self::PRIMARY.standard != 0,
            PointerButton::Auxiliary => self.standard & Self::AUXILIARY.standard != 0,
            PointerButton::Secondary => self.standard & Self::SECONDARY.standard != 0,
            PointerButton::Other(button) => self.other.contains(&button),
        }
    }

    /// Adds `button` to this set.
    pub fn insert(&mut self, button: PointerButton) {
        match button {
            PointerButton::Primary => self.standard |= Self::PRIMARY.standard,
            PointerButton::Auxiliary => self.standard |= Self::AUXILIARY.standard,
            PointerButton::Secondary => self.standard |= Self::SECONDARY.standard,
            PointerButton::Other(button) => {
                self.other.insert(button);
            }
        }
    }

    /// Removes `button` from this set.
    pub fn remove(&mut self, button: PointerButton) {
        match button {
            PointerButton::Primary => self.standard &= !Self::PRIMARY.standard,
            PointerButton::Auxiliary => self.standard &= !Self::AUXILIARY.standard,
            PointerButton::Secondary => self.standard &= !Self::SECONDARY.standard,
            PointerButton::Other(button) => {
                self.other.remove(&button);
            }
        }
    }
}
impl BitOr for PointerButtons {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            standard: self.standard | rhs.standard,
            other: self.other.union(&rhs.other).copied().collect(),
        }
    }
}

impl BitOrAssign for PointerButtons {
    fn bitor_assign(&mut self, rhs: Self) {
        self.standard |= rhs.standard;
        self.other.extend(rhs.other);
    }
}

impl BitAnd for PointerButtons {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self {
            standard: self.standard & rhs.standard,
            other: self.other.intersection(&rhs.other).copied().collect(),
        }
    }
}

impl BitAndAssign for PointerButtons {
    fn bitand_assign(&mut self, rhs: Self) {
        self.standard &= rhs.standard;
        self.other.retain(|button| rhs.other.contains(button));
    }
}

impl Not for PointerButtons {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self {
            standard: !self.standard & 0b111,
            other: (u16::MIN..=u16::MAX)
                .filter(|button| !self.other.contains(button))
                .collect(),
        }
    }
}

/// Keyboard modifier state.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifier keys are pressed.
    pub const NONE: Self = Self(0);
    /// Either shift key is pressed.
    pub const SHIFT: Self = Self(1 << 0);
    /// Either control key is pressed.
    pub const CONTROL: Self = Self(1 << 1);
    /// Either alt/option key is pressed.
    pub const ALT: Self = Self(1 << 2);
    /// Either command/super key is pressed.
    pub const META: Self = Self(1 << 3);

    /// Returns whether this state contains all `other` modifiers.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

/// Pointer data shared by pointer-button events.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerEvent {
    /// Device contact that produced the event.
    pub pointer_id: PointerId,
    /// Position relative to the window's content area.
    pub position: Point,
    /// Button that changed state, when applicable.
    pub button: Option<PointerButton>,
    /// All buttons pressed at the time of the event.
    pub buttons: PointerButtons,
    /// Keyboard modifiers pressed at the time of the event.
    pub modifiers: Modifiers,
}

/// The amount and unit of a pointer-wheel movement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelDelta {
    /// Delta expressed in logical pixels.
    Pixels(Point),
    /// Delta expressed in platform-defined lines.
    Lines(Point),
}

/// Pointer-wheel event data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelEvent {
    /// Pointer position relative to the window's content area.
    pub position: Point,
    /// Wheel movement.
    pub delta: WheelDelta,
    /// Keyboard modifiers pressed at the time of the event.
    pub modifiers: Modifiers,
}

/// Logical meaning of a key press.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Key {
    /// A text-producing key value.
    Character(String),
    /// A named, non-text key.
    Named(NamedKey),
    /// A key the platform could not identify.
    Unidentified,
}

/// A named non-text key value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NamedKey {
    /// Backspace.
    Backspace,
    /// Enter/return.
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// Space.
    Space,
    /// Arrow left.
    ArrowLeft,
    /// Arrow right.
    ArrowRight,
    /// Arrow up.
    ArrowUp,
    /// Arrow down.
    ArrowDown,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Delete.
    Delete,
}

/// Physical key position, independent of the active keyboard layout.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyCode {
    /// A physical key identified by the platform.
    Platform(u32),
    /// The physical key could not be identified.
    Unidentified,
}

/// Keyboard event data.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeyEvent {
    /// Logical key meaning.
    pub key: Key,
    /// Physical key position.
    pub code: KeyCode,
    /// Whether this event was produced by key repeat.
    pub repeat: bool,
    /// Keyboard modifiers pressed at the time of the event.
    pub modifiers: Modifiers,
}

/// A focus transition in the widget tree.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FocusChanged {
    /// Widget receiving focus, or `None` when focus leaves the tree.
    pub focused: Option<WidgetId>,
}

/// Framework-defined input sent through the widget event router.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// A pointer button was pressed.
    PointerDown(PointerEvent),
    /// A pointing-device contact moved.
    PointerMove(PointerEvent),
    /// A pointing-device contact entered a widget's hit-test bounds.
    PointerEnter(PointerEvent),
    /// A pointing-device contact left a widget's hit-test bounds.
    PointerLeave(PointerEvent),
    /// A pointer button was released.
    PointerUp(PointerEvent),
    /// The pointing device wheel moved.
    Wheel(WheelEvent),
    /// A keyboard key was pressed.
    KeyDown(KeyEvent),
    /// A keyboard key was released.
    KeyUp(KeyEvent),
    /// Text committed by the input method.
    TextInput(String),
    /// Focus changed within the widget tree.
    FocusChanged(FocusChanged),
}

#[cfg(test)]
mod tests {
    use super::{Modifiers, PointerButton, PointerButtons};

    #[test]
    fn modifier_and_button_sets_support_composition() {
        let modifiers = Modifiers::CONTROL | Modifiers::SHIFT;
        let buttons = PointerButtons::PRIMARY | PointerButtons::SECONDARY;

        assert!(modifiers.contains(Modifiers::CONTROL));
        assert!(!modifiers.contains(Modifiers::ALT));
        assert!(buttons.contains(&PointerButtons::PRIMARY));
        assert!(!buttons.contains(&PointerButtons::AUXILIARY));
    }

    #[test]
    fn button_sets_distinguish_additional_buttons() {
        let back = PointerButton::Other(4);
        let extra = PointerButton::Other(u16::MAX);
        let mut buttons = PointerButtons::PRIMARY;
        buttons.insert(back);
        buttons.insert(extra);

        assert!(buttons.contains_button(PointerButton::Primary));
        assert!(buttons.contains_button(back));
        assert!(buttons.contains_button(extra));
        assert!(!buttons.contains_button(PointerButton::Other(5)));

        buttons.remove(back);
        assert!(!buttons.contains_button(back));
        assert!(buttons.contains_button(extra));
    }
}
