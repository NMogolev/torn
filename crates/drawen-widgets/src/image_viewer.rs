use drawen_core::{
    Color, Constraints, InputEvent, Key, NamedKey, Point, PointerButton, PointerId, Rect, Size,
    WheelDelta,
};
use drawen_render::{PaintContext, RasterImage};
use drawen_ui::{
    Accessibility, AccessibilityRole, DssStyle, EventContext, EventPhase, EventStatus,
    LayoutContext, LayoutResult, Signal, StyleTarget, UiEnvironment, Widget,
};

use crate::image::image_size;

const DEFAULT_WIDTH: f32 = 320.0;
const DEFAULT_HEIGHT: f32 = 240.0;
const DEFAULT_MIN_ZOOM: f32 = 0.1;
const DEFAULT_MAX_ZOOM: f32 = 32.0;
const WHEEL_LINE_PIXELS: f32 = 40.0;
const ZOOM_STEP: f32 = 1.25;
const KEY_PAN: f32 = 48.0;
const CHECKER_SIZE: f32 = 16.0;

/// An interactive image surface with pointer panning and anchored wheel zoom.
///
/// At a zoom of `1.0` the image is fitted uniformly inside the viewer. Higher
/// zoom values enlarge it and enable panning; the pan offset is clamped so the
/// image never drifts completely out of view.
pub struct ImageViewer {
    image: RasterImage,
    zoom: f32,
    minimum_zoom: f32,
    maximum_zoom: f32,
    pan: Point,
    viewport: Size,
    dragging: Option<PointerId>,
    drag_origin: Point,
    pan_origin: Point,
    background: Option<Color>,
    checkerboard: bool,
    label: Option<String>,
    zoom_changed: Signal<f32>,
}

impl ImageViewer {
    /// Creates a viewer that fits `image` inside its bounds.
    #[must_use]
    pub fn new(image: RasterImage) -> Self {
        Self {
            image,
            zoom: 1.0,
            minimum_zoom: DEFAULT_MIN_ZOOM,
            maximum_zoom: DEFAULT_MAX_ZOOM,
            pan: Point::ZERO,
            viewport: Size::ZERO,
            dragging: None,
            drag_origin: Point::ZERO,
            pan_origin: Point::ZERO,
            background: None,
            checkerboard: false,
            label: None,
            zoom_changed: Signal::new(),
        }
    }

    /// Returns the displayed image.
    #[must_use]
    pub const fn image(&self) -> &RasterImage {
        &self.image
    }

    /// Returns the current zoom relative to the fitted image.
    #[must_use]
    pub const fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Returns the inclusive minimum zoom.
    #[must_use]
    pub const fn minimum_zoom(&self) -> f32 {
        self.minimum_zoom
    }

    /// Returns the inclusive maximum zoom.
    #[must_use]
    pub const fn maximum_zoom(&self) -> f32 {
        self.maximum_zoom
    }

    /// Returns the current pan offset in logical pixels.
    #[must_use]
    pub const fn pan(&self) -> Point {
        self.pan
    }

    /// Returns whether a checkerboard is painted behind transparent pixels.
    #[must_use]
    pub const fn checkerboard(&self) -> bool {
        self.checkerboard
    }

    /// Returns a signal emitted when the user changes the zoom.
    #[must_use]
    pub fn zoom_changed(&self) -> Signal<f32> {
        self.zoom_changed.clone()
    }

    /// Replaces the displayed image and resets the pan offset.
    pub fn set_image(&mut self, image: RasterImage) {
        self.image = image;
        self.pan = Point::ZERO;
        self.clamp_pan();
    }

    /// Sets the zoom relative to the fitted image without emitting [`Self::zoom_changed`].
    ///
    /// # Panics
    ///
    /// Panics when `zoom` is not finite.
    pub fn set_zoom(&mut self, zoom: f32) {
        assert!(zoom.is_finite(), "image viewer zoom must be finite");
        self.zoom = zoom.clamp(self.minimum_zoom, self.maximum_zoom);
        self.clamp_pan();
    }

    /// Restores the fitted zoom and centers the image.
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = Point::ZERO;
    }

    /// Returns this viewer with an explicit zoom range.
    ///
    /// # Panics
    ///
    /// Panics when either bound is non-finite or not positive, or when
    /// `minimum` exceeds `maximum`.
    #[must_use]
    pub fn with_zoom_range(mut self, minimum: f32, maximum: f32) -> Self {
        assert!(
            minimum.is_finite() && minimum > 0.0,
            "minimum image viewer zoom must be finite and positive"
        );
        assert!(
            maximum.is_finite() && maximum > 0.0,
            "maximum image viewer zoom must be finite and positive"
        );
        assert!(
            minimum <= maximum,
            "minimum image viewer zoom must not exceed the maximum"
        );
        self.minimum_zoom = minimum;
        self.maximum_zoom = maximum;
        self.zoom = self.zoom.clamp(minimum, maximum);
        self
    }

    /// Returns this viewer with an initial zoom.
    ///
    /// # Panics
    ///
    /// Panics when `zoom` is not finite.
    #[must_use]
    pub fn with_zoom(mut self, zoom: f32) -> Self {
        assert!(zoom.is_finite(), "image viewer zoom must be finite");
        self.zoom = zoom.clamp(self.minimum_zoom, self.maximum_zoom);
        self
    }

    /// Returns this viewer with a background color.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Returns this viewer with a transparency checkerboard.
    #[must_use]
    pub const fn with_checkerboard(mut self, enabled: bool) -> Self {
        self.checkerboard = enabled;
        self
    }

    /// Returns this viewer with an accessible name.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    fn base_scale_for(&self, viewport: Size) -> f32 {
        let content = image_size(&self.image);
        if content.width() <= 0.0
            || content.height() <= 0.0
            || viewport.width() <= 0.0
            || viewport.height() <= 0.0
        {
            return 1.0;
        }
        (viewport.width() / content.width())
            .min(viewport.height() / content.height())
            .max(f32::MIN_POSITIVE)
    }

    fn displayed_size(&self) -> Size {
        let content = image_size(&self.image);
        let scale = self.base_scale_for(self.viewport) * self.zoom;
        Size::new(content.width() * scale, content.height() * scale).unwrap_or(Size::ZERO)
    }

    fn clamp_pan(&mut self) {
        let displayed = self.displayed_size();
        let max_x = ((displayed.width() - self.viewport.width()) / 2.0).max(0.0);
        let max_y = ((displayed.height() - self.viewport.height()) / 2.0).max(0.0);
        self.pan.x = self.pan.x.clamp(-max_x, max_x);
        self.pan.y = self.pan.y.clamp(-max_y, max_y);
    }

    fn destination(&self, bounds: Rect) -> Rect {
        let content = image_size(&self.image);
        let scale = self.base_scale_for(bounds.size) * self.zoom;
        let width = content.width() * scale;
        let height = content.height() * scale;
        Rect::new(
            Point::new(
                bounds.origin.x + (bounds.size.width() - width) / 2.0 + self.pan.x,
                bounds.origin.y + (bounds.size.height() - height) / 2.0 + self.pan.y,
            ),
            Size::new(width, height).unwrap_or(Size::ZERO),
        )
    }

    fn local_center(&self) -> Point {
        Point::new(self.viewport.width() / 2.0, self.viewport.height() / 2.0)
    }

    fn zoom_at(&mut self, zoom: f32, anchor: Point, context: &mut EventContext<'_>) {
        let base = self.base_scale_for(self.viewport);
        let old_scale = base * self.zoom;
        let extent = image_size(&self.image);
        let center = self.local_center();
        let origin = Point::new(
            center.x - extent.width() * old_scale / 2.0 + self.pan.x,
            center.y - extent.height() * old_scale / 2.0 + self.pan.y,
        );
        let image_point = if old_scale > 0.0 {
            Point::new(
                (anchor.x - origin.x) / old_scale,
                (anchor.y - origin.y) / old_scale,
            )
        } else {
            Point::ZERO
        };

        self.zoom = zoom.clamp(self.minimum_zoom, self.maximum_zoom);
        let new_scale = base * self.zoom;
        let new_origin = Point::new(
            anchor.x - image_point.x * new_scale,
            anchor.y - image_point.y * new_scale,
        );
        self.pan = Point::new(
            new_origin.x - center.x + extent.width() * new_scale / 2.0,
            new_origin.y - center.y + extent.height() * new_scale / 2.0,
        );
        self.clamp_pan();
        context.request_redraw();
        self.zoom_changed.emit(&self.zoom);
    }

    fn pan_by(&mut self, delta: Point, context: &mut EventContext<'_>) {
        self.pan.x += delta.x;
        self.pan.y += delta.y;
        self.clamp_pan();
        context.request_redraw();
    }

    fn paint_checkerboard(context: &mut PaintContext<'_>, bounds: Rect, base: Color) {
        let light = base.lighten(0.08);
        let dark = base.darken(0.04);
        let right = bounds.origin.x + bounds.size.width();
        let bottom = bounds.origin.y + bounds.size.height();
        let mut row = 0_u32;
        let mut y = bounds.origin.y;
        while y < bottom {
            let mut column = 0_u32;
            let mut x = bounds.origin.x;
            while x < right {
                let color = if (row + column).is_multiple_of(2) {
                    light
                } else {
                    dark
                };
                let width = (right - x).min(CHECKER_SIZE);
                let height = (bottom - y).min(CHECKER_SIZE);
                context.fill_rect(
                    Rect::new(
                        Point::new(x, y),
                        Size::new(width, height).expect("clipped checker cell is finite"),
                    ),
                    color,
                );
                x += CHECKER_SIZE;
                column += 1;
            }
            y += CHECKER_SIZE;
            row += 1;
        }
    }
}

impl Widget for ImageViewer {
    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("image-viewer")
    }

    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        let max = constraints.max();
        let intrinsic = image_size(&self.image);
        let width = if max.width().is_finite() {
            max.width()
        } else if intrinsic.width() > 0.0 {
            intrinsic.width()
        } else {
            DEFAULT_WIDTH
        };
        let height = if max.height().is_finite() {
            max.height()
        } else if intrinsic.height() > 0.0 {
            intrinsic.height()
        } else {
            DEFAULT_HEIGHT
        };
        self.viewport = constraints
            .constrain(Size::new(width, height).expect("image viewer sizes are non-negative"));
        self.clamp_pan();
        LayoutResult::new(self.viewport)
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
        let background = self
            .background
            .or(style.background)
            .unwrap_or_else(|| theme.control_background());
        context.fill_rect(bounds, background);
        if self.checkerboard {
            Self::paint_checkerboard(context, bounds, background);
        }
        if self.image.width() > 0 && self.image.height() > 0 && !bounds.is_empty() {
            let destination = self.destination(bounds);
            context.with_clip(bounds, |context| {
                context.draw_image(self.image.clone(), destination);
            });
        }
        if let Some(border) = style.border {
            context.stroke_rect(bounds, border.width, border.color);
        }
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                self.dragging = Some(pointer.pointer_id);
                self.drag_origin = pointer.position;
                self.pan_origin = self.pan;
                context.capture_pointer(pointer.pointer_id);
                context.request_focus();
                EventStatus::Handled
            }
            InputEvent::PointerMove(pointer) if self.dragging == Some(pointer.pointer_id) => {
                self.pan = Point::new(
                    self.pan_origin.x + pointer.position.x - self.drag_origin.x,
                    self.pan_origin.y + pointer.position.y - self.drag_origin.y,
                );
                self.clamp_pan();
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer) if self.dragging == Some(pointer.pointer_id) => {
                self.dragging = None;
                context.release_pointer(pointer.pointer_id);
                EventStatus::Handled
            }
            InputEvent::PointerCancel(pointer) if self.dragging == Some(pointer.pointer_id) => {
                self.dragging = None;
                EventStatus::Handled
            }
            InputEvent::Wheel(wheel) => {
                let delta = match wheel.delta {
                    WheelDelta::Pixels(delta) => delta.y,
                    WheelDelta::Lines(delta) => delta.y * WHEEL_LINE_PIXELS,
                };
                if delta.abs() <= f32::EPSILON {
                    return EventStatus::Ignored;
                }
                let zoom = self.zoom * (-delta * 0.0015).exp();
                self.zoom_at(zoom, wheel.position, context);
                EventStatus::Handled
            }
            InputEvent::KeyDown(key) => self.handle_key(key, context),
            _ => EventStatus::Ignored,
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility(&self) -> Accessibility {
        let accessibility = Accessibility::new(AccessibilityRole::Image);
        match &self.label {
            Some(label) => accessibility.with_label(label.clone()),
            None => accessibility,
        }
    }
}

impl ImageViewer {
    fn handle_key(
        &mut self,
        key: &drawen_core::KeyEvent,
        context: &mut EventContext<'_>,
    ) -> EventStatus {
        let center = self.local_center();
        match &key.key {
            Key::Character(text) => match text.as_str() {
                "+" | "=" => {
                    self.zoom_at(self.zoom * ZOOM_STEP, center, context);
                    EventStatus::Handled
                }
                "-" | "_" => {
                    self.zoom_at(self.zoom / ZOOM_STEP, center, context);
                    EventStatus::Handled
                }
                "0" => {
                    self.reset_view();
                    context.request_redraw();
                    self.zoom_changed.emit(&self.zoom);
                    EventStatus::Handled
                }
                _ => EventStatus::Ignored,
            },
            Key::Named(named) => match named {
                NamedKey::ArrowLeft => {
                    self.pan_by(Point::new(KEY_PAN, 0.0), context);
                    EventStatus::Handled
                }
                NamedKey::ArrowRight => {
                    self.pan_by(Point::new(-KEY_PAN, 0.0), context);
                    EventStatus::Handled
                }
                NamedKey::ArrowUp => {
                    self.pan_by(Point::new(0.0, KEY_PAN), context);
                    EventStatus::Handled
                }
                NamedKey::ArrowDown => {
                    self.pan_by(Point::new(0.0, -KEY_PAN), context);
                    EventStatus::Handled
                }
                NamedKey::Home => {
                    self.reset_view();
                    context.request_redraw();
                    self.zoom_changed.emit(&self.zoom);
                    EventStatus::Handled
                }
                _ => EventStatus::Ignored,
            },
            Key::Unidentified => EventStatus::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use drawen_core::{Modifiers, PointerButtons, PointerEvent, WheelEvent};
    use drawen_render::{DisplayCommand, DisplayList};
    use drawen_ui::UiRuntime;

    use super::*;

    fn image(width: u32, height: u32) -> RasterImage {
        let pixels = vec![255; usize::try_from(width * height * 4).expect("small test image")];
        RasterImage::new(width, height, pixels).expect("valid RGBA image")
    }

    fn pointer(position: Point, button: Option<PointerButton>) -> PointerEvent {
        PointerEvent {
            pointer_id: PointerId(1),
            position,
            button,
            buttons: if button.is_some() {
                PointerButtons::PRIMARY
            } else {
                PointerButtons::NONE
            },
            modifiers: Modifiers::NONE,
        }
    }

    fn viewport(size: Size) -> Constraints {
        Constraints::tight(size).expect("valid viewport constraints")
    }

    fn image_origin(list: &DisplayList) -> f32 {
        list.commands()
            .iter()
            .find_map(|command| match command {
                DisplayCommand::DrawImage { destination, .. } => Some(destination.origin.x),
                _ => None,
            })
            .expect("viewer records an image draw")
    }

    #[test]
    fn fills_the_available_viewport() {
        let mut runtime = UiRuntime::new(ImageViewer::new(image(4, 2)));
        let expected = Size::new(100.0, 80.0).expect("valid size");
        let result = runtime
            .layout(viewport(expected))
            .expect("viewer layout succeeds");
        assert_eq!(result.size(), expected);
    }

    #[test]
    fn wheel_zooms_around_the_pointer() {
        let viewer = ImageViewer::new(image(10, 10));
        let observed = Rc::new(Cell::new(1.0));
        let _subscription = viewer.zoom_changed().subscribe({
            let observed = Rc::clone(&observed);
            move |zoom| observed.set(*zoom)
        });
        let mut runtime = UiRuntime::new(viewer);
        runtime
            .layout(viewport(Size::new(100.0, 100.0).expect("valid size")))
            .expect("viewer layout succeeds");
        let wheel = InputEvent::Wheel(WheelEvent {
            position: Point::new(50.0, 50.0),
            delta: WheelDelta::Pixels(Point::new(0.0, -100.0)),
            modifiers: Modifiers::NONE,
        });
        assert_eq!(runtime.dispatch_event(&wheel), EventStatus::Handled);
        assert!(observed.get() > 1.0);
    }

    #[test]
    fn dragging_pans_only_when_the_image_overflows() {
        let mut runtime = UiRuntime::new(ImageViewer::new(image(10, 10)).with_zoom(5.0));
        runtime
            .layout(viewport(Size::new(100.0, 100.0).expect("valid size")))
            .expect("viewer layout succeeds");
        let mut before = DisplayList::new();
        runtime
            .paint(&mut PaintContext::new(&mut before))
            .expect("initial paint succeeds");

        let down = InputEvent::PointerDown(pointer(
            Point::new(50.0, 50.0),
            Some(PointerButton::Primary),
        ));
        let moved = InputEvent::PointerMove(pointer(Point::new(70.0, 50.0), None));
        let up = InputEvent::PointerUp(pointer(
            Point::new(70.0, 50.0),
            Some(PointerButton::Primary),
        ));

        assert_eq!(runtime.dispatch_event(&down), EventStatus::Handled);
        assert_eq!(runtime.dispatch_event(&moved), EventStatus::Handled);
        assert_eq!(runtime.dispatch_event(&up), EventStatus::Handled);

        let mut after = DisplayList::new();
        runtime
            .paint(&mut PaintContext::new(&mut after))
            .expect("paint after drag succeeds");
        assert!((image_origin(&after) - image_origin(&before) - 20.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn reset_restores_the_fitted_view() {
        let mut viewer = ImageViewer::new(image(10, 10)).with_zoom(4.0);
        viewer.reset_view();
        assert!((viewer.zoom() - 1.0).abs() <= f32::EPSILON);
        assert_eq!(viewer.pan(), Point::ZERO);
    }

    #[test]
    fn records_a_clipped_image_draw() {
        let mut runtime = UiRuntime::new(ImageViewer::new(image(2, 2)));
        let mut list = DisplayList::new();
        runtime
            .layout(viewport(Size::new(100.0, 100.0).expect("valid size")))
            .expect("viewer layout succeeds");
        runtime
            .paint(&mut PaintContext::new(&mut list))
            .expect("viewer paint succeeds");
        assert!(
            list.commands()
                .iter()
                .any(|command| matches!(command, DisplayCommand::DrawImage { .. }))
        );
    }
}
