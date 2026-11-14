use drawen_core::{Constraints, InputEvent, Point, PointerButton, Rect, Size};
use drawen_render::{PaintContext, TextStyle};
use drawen_ui::{
    Accessibility, AccessibilityActions, AccessibilityRole, AccessibilityState, DssStyle,
    EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, Signal, StyleTarget,
    UiEnvironment, Widget,
};

const HORIZONTAL_PADDING: f32 = 10.0;
const VERTICAL_PADDING: f32 = 6.0;

/// A horizontal application menu bar with text menu headings.
///
/// Activating a heading emits its index through [`Self::activated`]. Popup menu
/// content remains application-owned for now, allowing commands and platform
/// menu integration to choose their own presentation.
pub struct MenuBar {
    menus: Vec<String>,
    activated: Signal<usize>,
    item_bounds: Vec<Rect>,
    size: Size,
}

impl MenuBar {
    /// Creates a menu bar from its top-level menu labels.
    #[must_use]
    pub fn new<T>(menus: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<String>,
    {
        Self {
            menus: menus.into_iter().map(Into::into).collect(),
            activated: Signal::new(),
            item_bounds: Vec::new(),
            size: Size::ZERO,
        }
    }

    /// Returns the menu labels in display order.
    #[must_use]
    pub fn menus(&self) -> &[String] {
        &self.menus
    }

    /// Returns a signal emitted with the activated menu index.
    #[must_use]
    pub fn activated(&self) -> Signal<usize> {
        self.activated.clone()
    }
}

impl Widget for MenuBar {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let environment = context.environment();
        let font = environment.font_for(&style);
        let font_size = font.pixel_size(environment.logical_dpi());
        let foreground = style.foreground_or(environment.theme().foreground());
        let text_style = TextStyle::new_with_font(font, foreground);
        let height = (font_size + VERTICAL_PADDING * 2.0).max(24.0);
        self.item_bounds.clear();
        let mut cursor = 0.0;
        for menu in &self.menus {
            let layout = environment.text_shaper().layout(menu, &text_style, None);
            let width = layout.size().width() + HORIZONTAL_PADDING * 2.0;
            self.item_bounds
                .push(Rect::new(Point::new(cursor, 0.0), size(width, height)));
            cursor += width;
        }
        self.size = constraints.constrain(size(cursor, height));
        LayoutResult::new(self.size)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("menu-bar")
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &DssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &DssStyle,
    ) {
        let theme = environment.theme();
        let radius = style.corner_radius.unwrap_or(0.0);
        let background = style.background.unwrap_or_else(|| theme.surface());
        if radius > 0.0 {
            context.fill_rounded_rect(bounds, radius, background);
        } else {
            context.fill_rect(bounds, background);
        }
        if let Some(border) = style.border {
            if radius > 0.0 {
                context.stroke_rounded_rect(bounds, radius, border.width, border.color);
            } else {
                context.stroke_rect(bounds, border.width, border.color);
            }
        } else {
            let border = theme.border();
            context.fill_rect(
                Rect::new(
                    Point::new(
                        bounds.origin.x,
                        bounds.origin.y + bounds.size.height() - 1.0,
                    ),
                    size(bounds.size.width(), 1.0),
                ),
                border,
            );
        }
        let font = environment.font_for(style);
        let foreground = style.foreground_or(theme.foreground());
        let text_style = TextStyle::new_with_font(font, foreground);
        context.with_clip(bounds, |context| {
            for (menu, item) in self.menus.iter().zip(&self.item_bounds) {
                let item = offset_rect(*item, bounds.origin);
                let text = environment.text_shaper().layout(
                    menu,
                    &text_style,
                    Some((item.size.width() - HORIZONTAL_PADDING * 2.0).max(0.0)),
                );
                context.draw_text(
                    text,
                    Point::new(
                        item.origin.x + HORIZONTAL_PADDING,
                        item.origin.y + VERTICAL_PADDING,
                    ),
                );
            }
        });
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        let InputEvent::PointerDown(pointer) = event else {
            return EventStatus::Ignored;
        };
        if pointer.button != Some(PointerButton::Primary) {
            return EventStatus::Ignored;
        }
        let Some(index) = self
            .item_bounds
            .iter()
            .position(|bounds| bounds.contains(pointer.position))
        else {
            return EventStatus::Ignored;
        };
        context.request_focus();
        self.activated.emit(&index);
        EventStatus::Handled
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        Accessibility::new(AccessibilityRole::MenuBar)
            .with_states(AccessibilityState::FOCUSABLE)
            .with_actions(AccessibilityActions::FOCUS)
    }
}

fn offset_rect(rect: Rect, offset: Point) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + offset.x, rect.origin.y + offset.y),
        rect.size,
    )
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("menu bar sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use drawen_core::{Modifiers, PointerButtons, PointerEvent, PointerId};
    use drawen_ui::UiRuntime;

    use super::*;

    #[test]
    fn emits_the_activated_menu_index() {
        let menu_bar = MenuBar::new(["File", "Edit"]);
        let activated = Rc::new(RefCell::new(Vec::new()));
        let _subscription = menu_bar.activated().subscribe({
            let activated = Rc::clone(&activated);
            move |index| activated.borrow_mut().push(*index)
        });
        let mut runtime = UiRuntime::new(menu_bar);
        runtime
            .layout(Constraints::UNBOUNDED)
            .expect("menu bar layout succeeds");

        let _ = runtime.dispatch_event(&InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position: Point::new(5.0, 5.0),
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        }));

        assert_eq!(*activated.borrow(), vec![0]);
    }
}
