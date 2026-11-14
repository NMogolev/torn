use drawen_core::{Color, Constraints, InputEvent, Point, PointerButton, Rect, Size};
use drawen_render::PaintContext;
use drawen_ui::{
    Border, ChildLayout, DssStyle, EventContext, EventPhase, EventStatus, LayoutContext,
    LayoutResult, ResizeEdge, StyleTarget, UiEnvironment, Widget, WindowCommand,
};

const DEFAULT_TITLE_BAR_HEIGHT: f32 = 38.0;
const DEFAULT_RESIZE_BORDER: f32 = 6.0;

/// A window-sized shell with a title-bar slot, a content slot, and optional
/// edge resizing for a client-side decorated window.
///
/// The first runtime-owned child is placed in the title-bar slot. The second
/// child fills the remaining content area. Additional children are ignored.
/// Primary presses within [`Self::resize_border`] of an edge or corner request
/// a native resize before the event reaches the title bar or content.
pub struct WindowFrame {
    title_bar_height: f32,
    resize_border: f32,
    resizable: bool,
    background: Option<Color>,
    border: Option<Border>,
    corner_radius: Option<f32>,
    size: Size,
}

impl WindowFrame {
    /// Creates a resizable window frame with a thirty-eight pixel title slot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            title_bar_height: DEFAULT_TITLE_BAR_HEIGHT,
            resize_border: DEFAULT_RESIZE_BORDER,
            resizable: true,
            background: None,
            border: None,
            corner_radius: None,
            size: Size::ZERO,
        }
    }

    /// Returns this frame with an explicit title-bar height.
    #[must_use]
    pub const fn with_title_bar_height(mut self, height: f32) -> Self {
        self.title_bar_height = height.max(0.0);
        self
    }

    /// Returns this frame with an explicit edge-resize thickness.
    #[must_use]
    pub const fn with_resize_border(mut self, thickness: f32) -> Self {
        self.resize_border = thickness.max(0.0);
        self
    }

    /// Returns this frame with edge resizing enabled or disabled.
    #[must_use]
    pub const fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Returns this frame with an explicit background color.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Returns this frame with a border.
    #[must_use]
    pub const fn with_border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }

    /// Returns this frame with rounded corners.
    #[must_use]
    pub const fn with_corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = Some(radius);
        self
    }

    /// Returns the configured title-bar height.
    #[must_use]
    pub const fn title_bar_height(&self) -> f32 {
        self.title_bar_height
    }

    /// Returns the configured edge-resize thickness.
    #[must_use]
    pub const fn resize_border(&self) -> f32 {
        self.resize_border
    }
}

impl Default for WindowFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for WindowFrame {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            640.0
        };
        let height = if max.height().is_finite() {
            max.height()
        } else {
            480.0
        };
        let frame_size = constraints.constrain(size(width, height));
        let title_height = self.title_bar_height.min(frame_size.height());
        let content_height = (frame_size.height() - title_height).max(0.0);
        let mut children = Vec::new();
        if context.child_count() > 0 {
            let (id, _) = context
                .layout_child(
                    0,
                    Constraints::tight(size(frame_size.width(), title_height))
                        .expect("frame sizes are finite"),
                )
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(id, Point::ZERO));
        }
        if context.child_count() > 1 {
            let (id, _) = context
                .layout_child(
                    1,
                    Constraints::tight(size(frame_size.width(), content_height))
                        .expect("frame sizes are finite"),
                )
                .expect("runtime child index is valid");
            children.push(ChildLayout::new(id, Point::new(0.0, title_height)));
        }
        self.size = frame_size;
        LayoutResult::with_children(frame_size, children)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("window-frame")
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
        let radius = self.corner_radius.or(style.corner_radius).unwrap_or(0.0);
        let background = self
            .background
            .or(style.background)
            .unwrap_or_else(|| environment.theme().surface());
        if radius > 0.0 {
            context.fill_rounded_rect(bounds, radius, background);
        } else {
            context.fill_rect(bounds, background);
        }
        if let Some(border) = self.border.or(style.border) {
            if radius > 0.0 {
                context.stroke_rounded_rect(bounds, radius, border.width, border.color);
            } else {
                context.stroke_rect(bounds, border.width, border.color);
            }
        }
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if !matches!(context.phase(), EventPhase::Capture | EventPhase::Target) {
            return EventStatus::Ignored;
        }
        let InputEvent::PointerDown(pointer) = event else {
            return EventStatus::Ignored;
        };
        if pointer.button != Some(PointerButton::Primary) || !self.resizable {
            return EventStatus::Ignored;
        }
        let Some(edge) = resize_edge(
            pointer.position,
            Rect::new(Point::ZERO, self.size),
            self.resize_border,
        ) else {
            return EventStatus::Ignored;
        };
        context.request_window_command(WindowCommand::StartResize(edge));
        context.stop_propagation();
        EventStatus::Handled
    }
}

fn resize_edge(position: Point, bounds: Rect, border: f32) -> Option<ResizeEdge> {
    if border <= 0.0 || !bounds.contains(position) {
        return None;
    }
    let left = position.x <= bounds.origin.x + border;
    let right = position.x >= bounds.right() - border;
    let top = position.y <= bounds.origin.y + border;
    let bottom = position.y >= bounds.bottom() - border;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeEdge::TopLeft),
        (_, true, true, _) => Some(ResizeEdge::TopRight),
        (true, _, _, true) => Some(ResizeEdge::BottomLeft),
        (_, true, _, true) => Some(ResizeEdge::BottomRight),
        (true, _, _, _) => Some(ResizeEdge::Left),
        (_, true, _, _) => Some(ResizeEdge::Right),
        (_, _, true, _) => Some(ResizeEdge::Top),
        (_, _, _, true) => Some(ResizeEdge::Bottom),
        _ => None,
    }
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("window frame sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use drawen_core::{Modifiers, PointerButtons, PointerEvent, PointerId};
    use drawen_ui::UiRuntime;

    use crate::TitleBar;

    use super::*;

    #[test]
    fn places_the_title_bar_above_content() {
        let mut runtime = UiRuntime::new(WindowFrame::new());
        let frame = runtime.root();
        let title = runtime
            .append_child(frame, TitleBar::new("Drawen"))
            .expect("frame exists");
        let content = runtime
            .append_child(frame, crate::Box::new().with_background(Color::BLACK))
            .expect("frame exists");
        runtime
            .layout(Constraints::tight(size(400.0, 300.0)).expect("valid constraints"))
            .expect("frame layout succeeds");

        assert_eq!(
            runtime.bounds(title).expect("title remains live"),
            Rect::new(Point::ZERO, size(400.0, DEFAULT_TITLE_BAR_HEIGHT))
        );
        assert_eq!(
            runtime.bounds(content).expect("content remains live"),
            Rect::new(
                Point::new(0.0, DEFAULT_TITLE_BAR_HEIGHT),
                size(400.0, 300.0 - DEFAULT_TITLE_BAR_HEIGHT)
            )
        );
    }

    #[test]
    fn requests_resize_from_edges_before_the_title_bar() {
        let mut runtime = UiRuntime::new(WindowFrame::new());
        let frame = runtime.root();
        runtime
            .append_child(frame, TitleBar::new("Drawen"))
            .expect("frame exists");
        runtime
            .append_child(frame, crate::Box::new())
            .expect("frame exists");
        runtime
            .layout(Constraints::tight(size(400.0, 300.0)).expect("valid constraints"))
            .expect("frame layout succeeds");

        let _ = runtime.dispatch_event(&pointer(Point::new(2.0, 2.0)));
        assert_eq!(
            runtime.take_window_commands(),
            vec![WindowCommand::StartResize(ResizeEdge::TopLeft)]
        );
    }

    #[test]
    fn starts_a_drag_from_the_title_bar_center() {
        let mut runtime = UiRuntime::new(WindowFrame::new());
        let frame = runtime.root();
        runtime
            .append_child(frame, TitleBar::new("Drawen"))
            .expect("frame exists");
        runtime
            .append_child(frame, crate::Box::new())
            .expect("frame exists");
        runtime
            .layout(Constraints::tight(size(400.0, 300.0)).expect("valid constraints"))
            .expect("frame layout succeeds");

        let _ = runtime.dispatch_event(&pointer(Point::new(200.0, 19.0)));

        assert_eq!(
            runtime.take_window_commands(),
            vec![WindowCommand::StartDrag]
        );
    }

    #[test]
    fn leaves_center_presses_for_children() {
        let mut runtime = UiRuntime::new(WindowFrame::new());
        let frame = runtime.root();
        runtime
            .append_child(frame, TitleBar::new("Drawen"))
            .expect("frame exists");
        runtime
            .append_child(frame, crate::Box::new())
            .expect("frame exists");
        runtime
            .layout(Constraints::tight(size(400.0, 300.0)).expect("valid constraints"))
            .expect("frame layout succeeds");

        let _ = runtime.dispatch_event(&pointer(Point::new(200.0, 100.0)));
        assert!(runtime.take_window_commands().is_empty());
    }

    fn pointer(position: Point) -> InputEvent {
        InputEvent::PointerDown(PointerEvent {
            pointer_id: PointerId(1),
            position,
            button: Some(PointerButton::Primary),
            buttons: PointerButtons::PRIMARY,
            modifiers: Modifiers::NONE,
        })
    }
}
