/// An opaque, generational identifier for a widget node.
///
/// The UI runtime creates these values. The generation distinguishes a newly
/// allocated node from a removed node that reused the same arena slot.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WidgetId {
    index: u32,
    generation: u32,
}

impl WidgetId {
    /// Creates an identifier from arena slot and generation values.
    #[must_use]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Returns the arena slot component.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the generation component.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}
#[cfg(test)]
mod tests {
    use super::WidgetId;

    #[test]
    fn generation_prevents_slot_reuse_from_comparing_equal() {
        assert_ne!(WidgetId::new(3, 0), WidgetId::new(3, 1));
    }
}
