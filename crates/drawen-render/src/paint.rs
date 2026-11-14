use drawen_core::{Affine, Color, Point, Rect};

use crate::{DisplayCommand, DisplayList, RasterImage, TextLayout, TextureId};

/// A recording interface that appends paint operations to a [`DisplayList`].
pub struct PaintContext<'a> {
    display_list: &'a mut DisplayList,
    transition: PaintTransition,
}

/// Eased interaction progress supplied to a widget during paint.
///
/// The runtime advances these values as the pointer hovers, presses, or focuses
/// a control, and widgets blend their idle, hover, pressed, and focused colors
/// with them. Values are already normalized to `0.0..=1.0` and use a smooth
/// ease-in-out curve, so widgets can mix colors directly.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PaintTransition {
    hover: f32,
    press: f32,
    focus: f32,
}

impl PaintTransition {
    /// No interaction progress; all values are zero.
    pub const NONE: Self = Self {
        hover: 0.0,
        press: 0.0,
        focus: 0.0,
    };

    /// Creates a transition from eased `hover`, `press`, and `focus` values.
    #[must_use]
    pub const fn new(hover: f32, press: f32, focus: f32) -> Self {
        Self {
            hover,
            press,
            focus,
        }
    }

    /// Returns the eased hover progress.
    #[must_use]
    pub const fn hover(self) -> f32 {
        self.hover
    }

    /// Returns the eased press progress.
    #[must_use]
    pub const fn press(self) -> f32 {
        self.press
    }

    /// Returns the eased focus progress.
    #[must_use]
    pub const fn focus(self) -> f32 {
        self.focus
    }

    /// Blends an idle color through its hovered and pressed variants.
    ///
    /// Hover is applied first, then press, so a pressed control retains its
    /// hover tint while the press fades in and out.
    #[must_use]
    pub fn control_color(self, idle: Color, hover: Color, pressed: Color) -> Color {
        idle.mix(hover, self.hover).mix(pressed, self.press)
    }

    /// Blends `idle` toward `focused` by the focus progress.
    #[must_use]
    pub fn focus_color(self, idle: Color, focused: Color) -> Color {
        idle.mix(focused, self.focus)
    }
}

impl<'a> PaintContext<'a> {
    /// Creates a context that records into `display_list`.
    pub fn new(display_list: &'a mut DisplayList) -> Self {
        Self {
            display_list,
            transition: PaintTransition::NONE,
        }
    }

    /// Replaces the interaction progress applied by widgets.
    ///
    /// `drawen_ui::UiRuntime` sets this before painting every node. Custom paint
    /// drivers can use it to preview or force a visual state.
    pub fn set_transition(&mut self, transition: PaintTransition) {
        self.transition = transition;
    }

    /// Returns the interaction progress for the widget currently being painted.
    #[must_use]
    pub const fn transition(&self) -> PaintTransition {
        self.transition
    }

    /// Returns the number of commands recorded in the target display list.
    #[must_use]
    pub fn command_count(&self) -> usize {
        self.display_list.len()
    }

    /// Appends previously recorded commands to this context in their original order.
    pub fn append_display_list(&mut self, display_list: &DisplayList) {
        self.display_list.extend_from(display_list);
    }

    /// Records a filled rectangle.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.display_list
            .push(DisplayCommand::FillRect { rect, color });
    }

    /// Records a filled rectangle with circular corners.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: f32, color: Color) {
        self.display_list.push(DisplayCommand::FillRoundedRect {
            rect,
            radius,
            color,
        });
    }

    /// Records a soft drop shadow cast by a rounded rectangle.
    ///
    /// `elevation` is the shadow's visual height in logical pixels. It drives
    /// both the downward offset and the width of the soft falloff. Widgets draw
    /// the shadow before the surface it belongs to.
    pub fn draw_shadow(&mut self, rect: Rect, radius: f32, elevation: f32, color: Color) {
        if !elevation.is_finite() || elevation <= 0.0 || color.alpha <= 0.0 {
            return;
        }
        let offset = elevation * 0.35;
        self.display_list.push(DisplayCommand::Shadow {
            rect: Rect::new(Point::new(rect.origin.x, rect.origin.y + offset), rect.size),
            radius,
            blur: elevation,
            color,
        });
    }

    /// Records a rectangular border centered on the rectangle's edge.
    pub fn stroke_rect(&mut self, rect: Rect, width: f32, color: Color) {
        self.display_list
            .push(DisplayCommand::StrokeRect { rect, width, color });
    }

    /// Records a rounded rectangular border centered on the shape's edge.
    pub fn stroke_rounded_rect(&mut self, rect: Rect, radius: f32, width: f32, color: Color) {
        self.display_list.push(DisplayCommand::StrokeRoundedRect {
            rect,
            radius,
            width,
            color,
        });
    }

    /// Saves the current clip and transform state.
    pub fn save(&mut self) {
        self.display_list.push(DisplayCommand::Save);
    }

    /// Restores the most recently saved clip and transform state.
    pub fn restore(&mut self) {
        self.display_list.push(DisplayCommand::Restore);
    }

    /// Executes `paint` with an isolated clip and transform state.
    ///
    /// The state is restored when `paint` returns, preventing an unbalanced
    /// scope from leaking into subsequent drawing operations.
    pub fn with_save(&mut self, paint: impl FnOnce(&mut Self)) {
        self.save();
        paint(self);
        self.restore();
    }

    /// Concatenates `transform` with the current transform.
    pub fn transform(&mut self, transform: Affine) {
        self.display_list
            .push(DisplayCommand::Transform { transform });
    }

    /// Translates all following operations in the current state scope.
    pub fn translate(&mut self, offset: Point) {
        self.transform(Affine::translate(offset.x, offset.y));
    }

    /// Starts a rectangular clip scope.
    pub fn push_clip(&mut self, rect: Rect) {
        self.save();
        self.clip_rect(rect);
    }

    /// Ends the most recently started clip scope.
    pub fn pop_clip(&mut self) {
        self.restore();
    }

    /// Intersects the current clip with `rect` in the current transform.
    pub fn clip_rect(&mut self, rect: Rect) {
        self.display_list.push(DisplayCommand::PushClip { rect });
    }

    /// Executes `paint` clipped to `rect` without leaking state afterward.
    pub fn with_clip(&mut self, rect: Rect, paint: impl FnOnce(&mut Self)) {
        self.with_save(|context| {
            context.clip_rect(rect);
            paint(context);
        });
    }

    /// Executes `paint` under `transform` without leaking state afterward.
    pub fn with_transform(&mut self, transform: Affine, paint: impl FnOnce(&mut Self)) {
        self.with_save(|context| {
            context.transform(transform);
            paint(context);
        });
    }

    /// Records a draw operation for already shaped and measured text.
    pub fn draw_text(&mut self, layout: TextLayout, origin: Point) {
        self.display_list
            .push(DisplayCommand::DrawText { layout, origin });
    }

    /// Records a raster image scaled into `destination`.
    pub fn draw_image(&mut self, image: RasterImage, destination: Rect) {
        self.display_list
            .push(DisplayCommand::DrawImage { image, destination });
    }

    /// Records a GPU texture scaled into `destination`.
    ///
    /// `texture` must have been registered with the GPU renderer before the
    /// display list is rendered.
    pub fn draw_texture(&mut self, texture: TextureId, destination: Rect) {
        self.display_list.push(DisplayCommand::DrawTexture {
            texture,
            destination,
        });
    }
}

#[cfg(test)]
mod tests {
    use drawen_core::{Affine, Color, Point, Rect, Size};

    use crate::{DisplayCommand, DisplayList, PaintContext, PaintTransition, TextureId};

    fn size(width: f32, height: f32) -> Size {
        Size::new(width, height).expect("valid test size")
    }

    #[test]
    fn records_paint_operations_in_order() {
        let rect = Rect::new(Point::new(1.0, 2.0), size(3.0, 4.0));
        let mut display_list = DisplayList::new();
        let mut paint = PaintContext::new(&mut display_list);

        paint.with_clip(rect, |context| context.fill_rect(rect, Color::BLACK));

        assert_eq!(
            display_list.commands(),
            &[
                DisplayCommand::Save,
                DisplayCommand::PushClip { rect },
                DisplayCommand::FillRect {
                    rect,
                    color: Color::BLACK,
                },
                DisplayCommand::Restore,
            ]
        );
    }

    #[test]
    fn records_shadows_with_a_soft_falloff() {
        let rect = Rect::new(Point::new(2.0, 4.0), size(10.0, 6.0));
        let mut display_list = DisplayList::new();

        PaintContext::new(&mut display_list).draw_shadow(rect, 3.0, 4.0, Color::BLACK);

        let [
            DisplayCommand::Shadow {
                rect: shadow,
                radius,
                blur,
                color,
            },
        ] = display_list.commands()
        else {
            panic!("expected one shadow command");
        };
        assert!((*radius - 3.0).abs() < f32::EPSILON);
        assert!((*blur - 4.0).abs() < f32::EPSILON);
        assert_eq!(*color, Color::BLACK);
        assert!((shadow.origin.y - rect.origin.y - 1.4).abs() < 0.001);
    }

    #[test]
    fn ignores_empty_shadows() {
        let rect = Rect::new(Point::ZERO, size(10.0, 6.0));
        let mut display_list = DisplayList::new();
        let mut paint = PaintContext::new(&mut display_list);

        paint.draw_shadow(rect, 3.0, 0.0, Color::BLACK);
        paint.draw_shadow(rect, 3.0, 4.0, Color::TRANSPARENT);
        paint.draw_shadow(rect, 3.0, f32::NAN, Color::BLACK);

        assert!(display_list.is_empty());
    }

    #[test]
    fn records_external_texture_draws() {
        let rect = Rect::new(Point::new(1.0, 2.0), size(3.0, 4.0));
        let texture = TextureId::new(9);
        let mut display_list = DisplayList::new();

        PaintContext::new(&mut display_list).draw_texture(texture, rect);

        assert_eq!(
            display_list.commands(),
            &[DisplayCommand::DrawTexture {
                texture,
                destination: rect,
            }]
        );
    }

    #[test]
    fn scopes_restore_transforms_before_the_next_operation() {
        let rect = Rect::new(Point::ZERO, size(1.0, 1.0));
        let mut display_list = DisplayList::new();
        let mut paint = PaintContext::new(&mut display_list);

        paint.with_transform(Affine::translate(4.0, 5.0), |context| {
            context.fill_rect(rect, Color::BLACK);
        });
        paint.fill_rect(rect, Color::WHITE);

        assert!(matches!(
            display_list.commands(),
            [
                DisplayCommand::Save,
                DisplayCommand::Transform { .. },
                DisplayCommand::FillRect { .. },
                DisplayCommand::Restore,
                DisplayCommand::FillRect {
                    color: Color::WHITE,
                    ..
                },
            ]
        ));
    }

    #[test]
    fn blends_control_colors_through_hover_and_press() {
        let idle = Color::rgb(0.0, 0.0, 0.0);
        let hover = Color::rgb(0.5, 0.5, 0.5);
        let pressed = Color::rgb(1.0, 1.0, 1.0);
        let transition = PaintTransition::new(1.0, 0.5, 0.0);

        let color = transition.control_color(idle, hover, pressed);

        assert!((color.red - 0.75).abs() < 0.001);
        assert_eq!(transition.focus_color(idle, pressed), idle);
    }

    #[test]
    fn paint_context_starts_without_transition_progress() {
        let mut display_list = DisplayList::new();
        let mut paint = PaintContext::new(&mut display_list);
        assert_eq!(paint.transition(), PaintTransition::NONE);

        paint.set_transition(PaintTransition::new(0.25, 0.5, 0.75));

        assert!((paint.transition().hover() - 0.25).abs() < f32::EPSILON);
        assert!((paint.transition().press() - 0.5).abs() < f32::EPSILON);
        assert!((paint.transition().focus() - 0.75).abs() < f32::EPSILON);
    }
}
