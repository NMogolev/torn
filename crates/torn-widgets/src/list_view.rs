use torn_core::{Constraints, InputEvent, Point, Size, WheelDelta};
use torn_ui::{
    ChildLayout, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Widget,
};

/// A clipped, vertically scrolling list that lays out only the visible rows.
///
/// Direct runtime-owned children are list rows in display order. Every row has
/// the fixed `item_extent` supplied at construction. One additional row before
/// and after the viewport is retained as overscan, avoiding visual gaps while
/// scrolling. Applications can retain as many rows as they need; off-screen
/// rows are not measured, painted, hit-tested, or included in focus traversal.
pub struct ListView {
    item_extent: f32,
    offset: f32,
    viewport: f32,
    content: f32,
}

impl ListView {
    /// Creates a vertical list whose rows are `item_extent` logical pixels tall.
    ///
    /// # Panics
    ///
    /// Panics when `item_extent` is not finite and positive.
    #[must_use]
    pub fn new(item_extent: f32) -> Self {
        assert!(
            item_extent.is_finite() && item_extent > 0.0,
            "list row extent must be finite and positive"
        );
        Self {
            item_extent,
            offset: 0.0,
            viewport: 0.0,
            content: 0.0,
        }
    }

    /// Returns the fixed height assigned to each row in logical pixels.
    #[must_use]
    pub const fn item_extent(&self) -> f32 {
        self.item_extent
    }

    /// Returns the current vertical scroll offset in logical pixels.
    #[must_use]
    pub const fn offset(&self) -> f32 {
        self.offset
    }

    /// Sets the vertical scroll offset, clamping it to the known content bounds.
    pub fn set_offset(&mut self, offset: f32) {
        self.offset = offset.clamp(0.0, (self.content - self.viewport).max(0.0));
    }
}

impl Widget for ListView {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let viewport = constraints.constrain(size(
            if max.width().is_finite() {
                max.width()
            } else {
                constraints.min().width()
            },
            if max.height().is_finite() {
                max.height()
            } else {
                constraints.min().height()
            },
        ));
        self.viewport = viewport.height();
        self.content = offset_for(context.child_count(), self.item_extent);
        self.set_offset(self.offset);

        let first_visible = row_index(self.offset / self.item_extent);
        let last_visible = row_index((self.offset + self.viewport) / self.item_extent);
        let first = first_visible.saturating_sub(1);
        let last = last_visible.saturating_add(1).min(context.child_count());
        let row_constraints = Constraints::tight(size(viewport.width(), self.item_extent))
            .expect("list row dimensions are finite");
        let mut children = Vec::with_capacity(last.saturating_sub(first));
        for index in first..last {
            let (id, _) = context
                .layout_child(index, row_constraints)
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(
                id,
                Point::new(0.0, offset_for(index, self.item_extent) - self.offset),
            ));
        }
        LayoutResult::with_children(viewport, children)
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Bubble {
            return EventStatus::Ignored;
        }
        let InputEvent::Wheel(wheel) = event else {
            return EventStatus::Ignored;
        };
        let delta = match wheel.delta {
            WheelDelta::Pixels(delta) => delta.y,
            WheelDelta::Lines(delta) => delta.y * 40.0,
        };
        let old = self.offset;
        self.set_offset(self.offset - delta);
        if (self.offset - old).abs() <= f32::EPSILON {
            return EventStatus::Ignored;
        }
        context.request_layout();
        EventStatus::Handled
    }

    fn clips_children(&self) -> bool {
        true
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("list layout sizes are non-negative and not NaN")
}

#[allow(clippy::cast_precision_loss)]
fn offset_for(index: usize, item_extent: f32) -> f32 {
    // Coordinates use f32 throughout the layout contract. At magnitudes where
    // an index cannot be represented exactly, individual logical pixels are
    // likewise no longer distinguishable, so this is the appropriate boundary.
    index as f32 * item_extent
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn row_index(offset: f32) -> usize {
    // Scroll offsets are always finite and non-negative after `set_offset`.
    offset as usize
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use torn_core::{
        Modifiers, PointerButton, PointerButtons, PointerEvent, PointerId, WheelEvent,
    };
    use torn_render::{DisplayList, PaintContext};
    use torn_ui::UiRuntime;

    use super::*;

    struct Row {
        index: usize,
        layouts: Rc<Cell<usize>>,
        targets: Rc<std::cell::RefCell<Vec<usize>>>,
    }

    impl Widget for Row {
        fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
            self.layouts.set(self.layouts.get() + 1);
            LayoutResult::new(constraints.constrain(Size::ZERO))
        }

        fn handle_event(
            &mut self,
            context: &mut EventContext<'_>,
            event: &InputEvent,
        ) -> EventStatus {
            if context.phase() == EventPhase::Target && matches!(event, InputEvent::PointerDown(_))
            {
                self.targets.borrow_mut().push(self.index);
                EventStatus::Handled
            } else {
                EventStatus::Ignored
            }
        }
    }

    #[test]
    fn virtualizes_rows_and_relayouts_after_scrolling() {
        let layouts = Rc::new(Cell::new(0));
        let targets = Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(ListView::new(10.0));
        let list = runtime.root();
        for index in 0..10 {
            runtime
                .append_child(
                    list,
                    Row {
                        index,
                        layouts: Rc::clone(&layouts),
                        targets: Rc::clone(&targets),
                    },
                )
                .expect("list exists");
        }
        let viewport = Constraints::tight(size(100.0, 20.0)).expect("viewport is finite");
        runtime.layout(viewport).expect("initial layout succeeds");
        assert_eq!(layouts.get(), 3);

        let wheel = InputEvent::Wheel(WheelEvent {
            position: Point::new(5.0, 5.0),
            delta: WheelDelta::Pixels(Point::new(0.0, -20.0)),
            modifiers: Modifiers::NONE,
        });
        assert_eq!(runtime.dispatch_event(&wheel), EventStatus::Handled);
        assert!(runtime.last_layout().is_none());

        let mut display_list = DisplayList::new();
        runtime
            .paint(&mut PaintContext::new(&mut display_list))
            .expect("deferred layout and paint succeed");
        assert_eq!(layouts.get(), 7);

        let down = InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(5.0, 5.0),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(runtime.dispatch_event(&down), EventStatus::Handled);
        assert_eq!(*targets.borrow(), vec![2]);
    }
}
