use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use torn_core::{Affine, Color, Constraints, InputEvent, Insets, Point, PointerButton, Rect, Size};
use torn_render::{PaintContext, TextStyle};
use torn_ui::{
    Border, EventContext, EventPhase, EventStatus, LayoutContext, LayoutResult, StyleTarget,
    TssStyle, UiEnvironment, Widget, WindowCommand,
};

use crate::Alignment;

const DEFAULT_WIDTH: f32 = 640.0;
const DEFAULT_HEIGHT: f32 = 38.0;
const DEFAULT_BUTTON_WIDTH: f32 = 46.0;
const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(400);
const DOUBLE_CLICK_DISTANCE: f32 = 6.0;
const GLYPH_SIZE: f32 = 10.0;
const GLYPH_STROKE: f32 = 1.4;

/// A window control rendered by a [`TitleBar`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WindowControl {
    /// Minimize the window.
    Minimize,
    /// Toggle between maximized and restored geometry.
    Maximize,
    /// Toggle borderless fullscreen mode.
    ToggleFullscreen,
    /// Ask the application to close the window.
    Close,
}

/// Shared, observable maximized and fullscreen state for a custom title bar.
///
/// The host application updates this state from its platform events. A title
/// bar also updates it immediately when the user clicks a window control, so
/// the painted glyph reacts without waiting for the next native event.
#[derive(Clone, Debug, Default)]
pub struct WindowChromeState {
    maximized: Rc<Cell<bool>>,
    fullscreen: Rc<Cell<bool>>,
}

impl WindowChromeState {
    /// Creates state with both maximized and fullscreen disabled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether the window is currently maximized.
    #[must_use]
    pub fn maximized(&self) -> bool {
        self.maximized.get()
    }

    /// Updates the maximized state.
    pub fn set_maximized(&self, maximized: bool) {
        self.maximized.set(maximized);
    }

    /// Returns whether the window is currently fullscreen.
    #[must_use]
    pub fn fullscreen(&self) -> bool {
        self.fullscreen.get()
    }

    /// Updates the fullscreen state.
    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.fullscreen.set(fullscreen);
    }
}

/// Locally overridable visual properties for a [`TitleBar`].
///
/// The four independent flags (`show_title`, `show_icon`, `draggable`, and
/// `double_click_maximizes`) are deliberately separate configuration switches
/// rather than a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq)]
pub struct TitleBarStyle {
    /// Preferred title-bar height in logical pixels.
    pub height: f32,
    /// Content padding. Window controls are placed relative to the right or
    /// left edge of this padding.
    pub padding: Insets,
    /// Alignment of the title block inside the free space between controls.
    pub title_alignment: Alignment,
    /// Side of the bar that owns the window controls.
    pub controls_alignment: Alignment,
    /// Width of every window-control button.
    pub button_width: f32,
    /// Gap between adjacent window-control buttons.
    pub button_spacing: f32,
    /// Gap between the icon, title, subtitle, and window controls.
    pub icon_gap: f32,
    /// Whether the title text is painted.
    pub show_title: bool,
    /// Whether the optional icon is painted.
    pub show_icon: bool,
    /// Whether pressing the empty title-bar area starts a native window drag.
    pub draggable: bool,
    /// Whether a double click on the empty title-bar area toggles maximize.
    pub double_click_maximizes: bool,
    /// Background color. Resolves from the theme when absent.
    pub background: Option<Color>,
    /// Title and icon color. Resolves from the theme when absent.
    pub foreground: Option<Color>,
    /// Subtitle color. Resolves from the theme when absent.
    pub muted_foreground: Option<Color>,
    /// Hover background for non-close window controls.
    pub hover_background: Option<Color>,
    /// Pressed background for non-close window controls.
    pub pressed_background: Option<Color>,
    /// Hover background for the close control.
    pub close_hover_background: Option<Color>,
    /// Pressed background for the close control.
    pub close_pressed_background: Option<Color>,
    /// Corner radius of the title-bar surface.
    pub corner_radius: Option<f32>,
    /// Optional border around the title-bar surface.
    pub border: Option<Border>,
}

impl Default for TitleBarStyle {
    fn default() -> Self {
        Self {
            height: DEFAULT_HEIGHT,
            padding: Insets::new(0.0, 8.0, 0.0, 12.0),
            title_alignment: Alignment::Start,
            controls_alignment: Alignment::End,
            button_width: DEFAULT_BUTTON_WIDTH,
            button_spacing: 0.0,
            icon_gap: 8.0,
            show_title: true,
            show_icon: true,
            draggable: true,
            double_click_maximizes: true,
            background: None,
            foreground: None,
            muted_foreground: None,
            hover_background: None,
            pressed_background: None,
            close_hover_background: None,
            close_pressed_background: None,
            corner_radius: None,
            border: None,
        }
    }
}

/// A configurable client-side title bar with window controls.
///
/// The bar paints its own title, optional icon and subtitle, and a configurable
/// list of minimize, maximize/restore, fullscreen, and close controls. Pointer
/// presses on controls and on the draggable area are reported as
/// [`WindowCommand`] values through
/// [`torn_ui::EventContext::request_window_command`]. The host application
/// drains them with [`torn_ui::UiRuntime::take_window_commands`] and forwards
/// them to its platform adapter.
///
/// Append at most one child to replace the built-in title block. The child is
/// laid out in the free space between the leading and trailing controls, which
/// allows custom title-bar content such as tabs or a search field.
pub struct TitleBar {
    title: String,
    subtitle: Option<String>,
    icon: Option<String>,
    style: TitleBarStyle,
    controls: Vec<WindowControl>,
    state: WindowChromeState,
    hovered_control: Option<usize>,
    pressed_control: Option<usize>,
    last_click: Option<(Instant, Point)>,
    button_bounds: Vec<Rect>,
    title_area: Rect,
    has_child: bool,
    size: Size,
}

impl TitleBar {
    /// Creates a title bar with the standard minimize, maximize, fullscreen,
    /// and close controls.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            icon: None,
            style: TitleBarStyle::default(),
            controls: vec![
                WindowControl::Minimize,
                WindowControl::Maximize,
                WindowControl::ToggleFullscreen,
                WindowControl::Close,
            ],
            state: WindowChromeState::new(),
            hovered_control: None,
            pressed_control: None,
            last_click: None,
            button_bounds: Vec::new(),
            title_area: Rect::ZERO,
            has_child: false,
            size: Size::ZERO,
        }
    }

    /// Returns this title bar with an optional icon glyph.
    #[must_use]
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Returns this title bar with an optional subtitle.
    #[must_use]
    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Returns this title bar with `style` applied locally.
    #[must_use]
    pub fn with_style(mut self, style: TitleBarStyle) -> Self {
        self.set_style(style);
        self
    }

    /// Replaces the local visual style.
    pub fn set_style(&mut self, style: TitleBarStyle) {
        self.style = style;
    }

    /// Returns the local visual style.
    #[must_use]
    pub const fn style(&self) -> &TitleBarStyle {
        &self.style
    }

    /// Replaces the title text.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Returns the title text.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Replaces the optional subtitle.
    pub fn set_subtitle(&mut self, subtitle: Option<String>) {
        self.subtitle = subtitle;
    }

    /// Replaces the optional icon glyph.
    pub fn set_icon(&mut self, icon: Option<String>) {
        self.icon = icon;
    }

    /// Returns this title bar with an explicit window-control order.
    #[must_use]
    pub fn with_controls(mut self, controls: impl IntoIterator<Item = WindowControl>) -> Self {
        self.controls = controls.into_iter().collect();
        self
    }

    /// Returns this title bar with `control` appended.
    #[must_use]
    pub fn with_control(mut self, control: WindowControl) -> Self {
        if !self.controls.contains(&control) {
            self.controls.push(control);
        }
        self
    }

    /// Returns this title bar without `control`.
    #[must_use]
    pub fn without_control(mut self, control: WindowControl) -> Self {
        self.controls.retain(|item| *item != control);
        self
    }

    /// Returns this title bar with an explicit height.
    #[must_use]
    pub const fn with_height(mut self, height: f32) -> Self {
        self.style.height = height.max(0.0);
        self
    }

    /// Returns this title bar with explicit content padding.
    #[must_use]
    pub const fn with_padding(mut self, padding: Insets) -> Self {
        self.style.padding = padding;
        self
    }

    /// Returns this title bar with an explicit title alignment.
    #[must_use]
    pub const fn with_title_alignment(mut self, alignment: Alignment) -> Self {
        self.style.title_alignment = alignment;
        self
    }

    /// Returns this title bar with controls on the requested side.
    #[must_use]
    pub const fn with_controls_alignment(mut self, alignment: Alignment) -> Self {
        self.style.controls_alignment = alignment;
        self
    }

    /// Returns this title bar with an explicit window-control width.
    #[must_use]
    pub const fn with_button_width(mut self, width: f32) -> Self {
        self.style.button_width = width.max(0.0);
        self
    }

    /// Returns this title bar with an explicit background color.
    #[must_use]
    pub const fn with_background(mut self, color: Color) -> Self {
        self.style.background = Some(color);
        self
    }

    /// Returns this title bar with an explicit foreground color.
    #[must_use]
    pub const fn with_foreground(mut self, color: Color) -> Self {
        self.style.foreground = Some(color);
        self
    }

    /// Returns this title bar with an explicit control hover color.
    #[must_use]
    pub const fn with_hover_background(mut self, color: Color) -> Self {
        self.style.hover_background = Some(color);
        self
    }

    /// Returns this title bar with an explicit control pressed color.
    #[must_use]
    pub const fn with_pressed_background(mut self, color: Color) -> Self {
        self.style.pressed_background = Some(color);
        self
    }

    /// Returns this title bar with an explicit close-control hover color.
    #[must_use]
    pub const fn with_close_hover_background(mut self, color: Color) -> Self {
        self.style.close_hover_background = Some(color);
        self
    }

    /// Returns this title bar with an explicit corner radius.
    #[must_use]
    pub const fn with_corner_radius(mut self, radius: f32) -> Self {
        self.style.corner_radius = Some(radius);
        self
    }

    /// Returns this title bar with a border.
    #[must_use]
    pub const fn with_border(mut self, border: Border) -> Self {
        self.style.border = Some(border);
        self
    }

    /// Returns this title bar with drag-on-press enabled or disabled.
    #[must_use]
    pub const fn with_draggable(mut self, draggable: bool) -> Self {
        self.style.draggable = draggable;
        self
    }

    /// Returns this title bar with double-click maximize enabled or disabled.
    #[must_use]
    pub const fn with_double_click_maximizes(mut self, enabled: bool) -> Self {
        self.style.double_click_maximizes = enabled;
        self
    }

    /// Returns this title bar with shared maximized/fullscreen state.
    #[must_use]
    pub fn with_state(mut self, state: WindowChromeState) -> Self {
        self.state = state;
        self
    }

    /// Returns the shared maximized/fullscreen state.
    #[must_use]
    pub fn state(&self) -> WindowChromeState {
        self.state.clone()
    }

    /// Returns the window-control order.
    #[must_use]
    pub fn controls(&self) -> &[WindowControl] {
        &self.controls
    }

    fn control_at(&self, position: Point) -> Option<usize> {
        self.button_bounds
            .iter()
            .position(|bounds| bounds.contains(position))
    }

    fn update_hover(&mut self, position: Point, context: &mut EventContext<'_>) {
        let hovered = self.control_at(position);
        if hovered != self.hovered_control {
            self.hovered_control = hovered;
            context.request_redraw();
        }
    }

    fn activate(&mut self, control: WindowControl, context: &mut EventContext<'_>) {
        let command = match control {
            WindowControl::Minimize => WindowCommand::Minimize,
            WindowControl::Maximize => {
                self.state.set_maximized(!self.state.maximized());
                WindowCommand::ToggleMaximize
            }
            WindowControl::ToggleFullscreen => {
                self.state.set_fullscreen(!self.state.fullscreen());
                WindowCommand::ToggleFullscreen
            }
            WindowControl::Close => WindowCommand::Close,
        };
        context.request_window_command(command);
        context.request_redraw();
    }

    fn is_double_click(&self, now: Instant, position: Point) -> bool {
        self.style.double_click_maximizes
            && self.last_click.is_some_and(|(previous, last)| {
                now.duration_since(previous) <= DOUBLE_CLICK_TIME
                    && distance(last, position) <= DOUBLE_CLICK_DISTANCE
            })
    }
}

impl Default for TitleBar {
    fn default() -> Self {
        Self::new("")
    }
}

impl Widget for TitleBar {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let style = context.style();
        let padding = style.padding.unwrap_or(self.style.padding);
        let max = constraints.max();
        let width = if max.width().is_finite() {
            max.width()
        } else {
            DEFAULT_WIDTH
        };
        let height = if max.height().is_finite() {
            self.style.height.min(max.height())
        } else {
            self.style.height
        };
        let bar_size = constraints.constrain(size(width, height));

        let button_width = self.style.button_width.max(0.0);
        let spacing = self.style.button_spacing.max(0.0);
        let count = self.controls.len();
        let total = if count == 0 {
            0.0
        } else {
            count_as_f32(count) * button_width + count_as_f32(count - 1) * spacing
        };
        let controls_start = match self.style.controls_alignment {
            Alignment::Start => padding.left,
            Alignment::Center => ((bar_size.width() - total) / 2.0).max(padding.left),
            Alignment::End => (bar_size.width() - padding.right - total).max(padding.left),
        };
        self.button_bounds.clear();
        for index in 0..count {
            let x = controls_start + count_as_f32(index) * (button_width + spacing);
            self.button_bounds.push(Rect::new(
                Point::new(x, 0.0),
                size(button_width, bar_size.height()),
            ));
        }

        let gap = self.style.icon_gap.max(0.0);
        let leading = if self.style.controls_alignment == Alignment::Start {
            controls_start + total + gap
        } else {
            padding.left
        };
        let trailing = if self.style.controls_alignment == Alignment::End {
            padding.right + total + gap
        } else {
            padding.right
        };
        let area_width = (bar_size.width() - leading - trailing).max(0.0);
        self.title_area = Rect::new(
            Point::new(leading, 0.0),
            size(area_width, bar_size.height()),
        );
        self.has_child = context.child_count() > 0;
        let mut children = Vec::new();
        if self.has_child {
            let (id, layout) = context
                .layout_child(0, Constraints::loose(size(area_width, bar_size.height())))
                .expect("runtime child index is valid");
            let child = layout.size();
            let x = match self.style.title_alignment {
                Alignment::Start => leading,
                Alignment::Center => leading + ((area_width - child.width()) / 2.0).max(0.0),
                Alignment::End => leading + (area_width - child.width()).max(0.0),
            };
            let y = ((bar_size.height() - child.height()) / 2.0).max(0.0);
            children.push(torn_ui::ChildLayout::new(id, Point::new(x, y)));
        }

        self.size = bar_size;
        LayoutResult::with_children(bar_size, children)
    }

    fn style_target(&self) -> StyleTarget {
        StyleTarget::new("title-bar")
    }

    fn paint(&self, context: &mut PaintContext<'_>, environment: &UiEnvironment, bounds: Rect) {
        self.paint_styled(context, environment, bounds, &TssStyle::default());
    }

    fn paint_styled(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        bounds: Rect,
        style: &TssStyle,
    ) {
        let theme = environment.theme();
        let radius = self
            .style
            .corner_radius
            .or(style.corner_radius)
            .unwrap_or(0.0);
        let background = self
            .style
            .background
            .or(style.background)
            .unwrap_or_else(|| theme.title_bar_background());
        if radius > 0.0 {
            context.fill_rounded_rect(bounds, radius, background);
        } else {
            context.fill_rect(bounds, background);
        }
        if let Some(border) = self.style.border.or(style.border) {
            if radius > 0.0 {
                context.stroke_rounded_rect(bounds, radius, border.width, border.color);
            } else {
                context.stroke_rect(bounds, border.width, border.color);
            }
        }

        if !self.controls.is_empty() {
            context.with_clip(bounds, |context| {
                for (index, control) in self.controls.iter().enumerate() {
                    let Some(local) = self.button_bounds.get(index) else {
                        continue;
                    };
                    let rect = offset_rect(*local, bounds.origin);
                    self.paint_control(context, environment, style, *control, index, rect);
                }
            });
        }

        if self.has_child {
            return;
        }
        let area = offset_rect(self.title_area, bounds.origin);
        if area.size.width() <= 0.0 || area.size.height() <= 0.0 {
            return;
        }
        let font = environment.font_for(style);
        let foreground = self
            .style
            .foreground
            .or(style.foreground)
            .unwrap_or_else(|| theme.title_bar_foreground());
        let muted = self
            .style
            .muted_foreground
            .unwrap_or_else(|| theme.muted_foreground());
        let gap = self.style.icon_gap.max(0.0);
        let mut cursor = area.origin.x;
        let right = area.origin.x + area.size.width();

        context.with_clip(area, |context| {
            if self.style.show_icon
                && let Some(icon) = &self.icon
            {
                let icon_font = font
                    .clone()
                    .with_pixel_size(font.pixel_size(environment.logical_dpi()) + 2.0);
                let layout = environment.text_shaper().layout(
                    icon,
                    &TextStyle::new_with_font(icon_font, foreground),
                    Some((right - cursor).max(0.0)),
                );
                let y = area.origin.y + (area.size.height() - layout.size().height()) / 2.0;
                context.draw_text(layout.clone(), Point::new(cursor, y));
                cursor += layout.size().width() + gap;
            }

            if self.style.show_title && !self.title.is_empty() {
                let layout = environment.text_shaper().layout(
                    &self.title,
                    &TextStyle::new_with_font(font.clone(), foreground),
                    Some((right - cursor).max(0.0)),
                );
                let y = area.origin.y + (area.size.height() - layout.size().height()) / 2.0;
                context.draw_text(layout.clone(), Point::new(cursor, y));
                cursor += layout.size().width();
            }

            if let Some(subtitle) = &self.subtitle
                && cursor < right
            {
                let subtitle_font = font
                    .clone()
                    .with_pixel_size(font.pixel_size(environment.logical_dpi()) * 0.85);
                let layout = environment.text_shaper().layout(
                    subtitle,
                    &TextStyle::new_with_font(subtitle_font, muted),
                    Some((right - cursor).max(0.0)),
                );
                let y = area.origin.y + (area.size.height() - layout.size().height()) / 2.0;
                context.draw_text(layout, Point::new(cursor, y));
            }
        });
    }

    fn handle_event(&mut self, context: &mut EventContext<'_>, event: &InputEvent) -> EventStatus {
        if context.phase() != EventPhase::Target {
            return EventStatus::Ignored;
        }
        match event {
            InputEvent::PointerDown(pointer) if pointer.button == Some(PointerButton::Primary) => {
                if let Some(index) = self.control_at(pointer.position) {
                    self.pressed_control = Some(index);
                    self.hovered_control = Some(index);
                    context.capture_pointer(pointer.pointer_id);
                    context.request_redraw();
                    return EventStatus::Handled;
                }
                if self.style.draggable || self.style.double_click_maximizes {
                    let now = Instant::now();
                    if self.is_double_click(now, pointer.position) {
                        self.last_click = None;
                        self.state.set_maximized(!self.state.maximized());
                        context.request_window_command(WindowCommand::ToggleMaximize);
                        context.request_redraw();
                        return EventStatus::Handled;
                    }
                    self.last_click = Some((now, pointer.position));
                    if self.style.draggable {
                        context.request_window_command(WindowCommand::StartDrag);
                    }
                    return EventStatus::Handled;
                }
                EventStatus::Ignored
            }
            InputEvent::PointerMove(pointer) | InputEvent::PointerEnter(pointer) => {
                self.update_hover(pointer.position, context);
                EventStatus::Handled
            }
            InputEvent::PointerLeave(_) => {
                if self.hovered_control.take().is_some() {
                    context.request_redraw();
                }
                EventStatus::Handled
            }
            InputEvent::PointerUp(pointer)
                if pointer.button == Some(PointerButton::Primary)
                    && self.pressed_control.is_some() =>
            {
                let index = self.pressed_control.take().expect("pressed control exists");
                context.release_pointer(pointer.pointer_id);
                if self.control_at(pointer.position) == Some(index) {
                    self.activate(self.controls[index], context);
                }
                context.request_redraw();
                EventStatus::Handled
            }
            InputEvent::PointerCancel(_) if self.pressed_control.is_some() => {
                self.pressed_control = None;
                context.request_redraw();
                EventStatus::Handled
            }
            _ => EventStatus::Ignored,
        }
    }
}

impl TitleBar {
    fn paint_control(
        &self,
        context: &mut PaintContext<'_>,
        environment: &UiEnvironment,
        style: &TssStyle,
        control: WindowControl,
        index: usize,
        rect: Rect,
    ) {
        let theme = environment.theme();
        let hovered = self.hovered_control == Some(index);
        let pressed = self.pressed_control == Some(index) && hovered;
        let close = control == WindowControl::Close;
        let background = if pressed {
            if close {
                self.style
                    .close_pressed_background
                    .or(self.style.close_hover_background)
                    .or_else(|| Some(theme.danger()))
            } else {
                self.style
                    .pressed_background
                    .or_else(|| Some(theme.control_pressed_background()))
            }
        } else if hovered {
            if close {
                self.style
                    .close_hover_background
                    .or_else(|| Some(theme.danger()))
            } else {
                self.style
                    .hover_background
                    .or_else(|| Some(theme.title_bar_hover_background()))
            }
        } else {
            None
        };
        if let Some(background) = background {
            context.fill_rect(rect, background);
        }
        let foreground = self
            .style
            .foreground
            .or(style.foreground)
            .unwrap_or_else(|| theme.title_bar_foreground());
        let color = if close && (hovered || pressed) {
            Color::WHITE
        } else {
            foreground
        };
        let center = Point::new(
            rect.origin.x + rect.size.width() / 2.0,
            rect.origin.y + rect.size.height() / 2.0,
        );
        paint_control_glyph(context, control, center, color, self.state.maximized());
    }
}

fn paint_control_glyph(
    context: &mut PaintContext<'_>,
    control: WindowControl,
    center: Point,
    color: Color,
    maximized: bool,
) {
    let half = GLYPH_SIZE / 2.0;
    let left = center.x - half;
    let right = center.x + half;
    let top = center.y - half;
    let bottom = center.y + half;
    match control {
        WindowControl::Minimize => {
            context.fill_rect(
                Rect::new(
                    Point::new(left, center.y - GLYPH_STROKE / 2.0),
                    size(GLYPH_SIZE, GLYPH_STROKE),
                ),
                color,
            );
        }
        WindowControl::Maximize => {
            if maximized {
                let back = Rect::new(Point::new(left + 2.0, top), size(8.0, 8.0));
                let front = Rect::new(Point::new(left, top + 2.0), size(8.0, 8.0));
                context.stroke_rect(back, GLYPH_STROKE, color);
                context.stroke_rect(front, GLYPH_STROKE, color);
            } else {
                context.stroke_rect(
                    Rect::new(Point::new(left, top), size(GLYPH_SIZE, GLYPH_SIZE)),
                    GLYPH_STROKE,
                    color,
                );
            }
        }
        WindowControl::ToggleFullscreen => {
            let arm = 4.0;
            let thickness = GLYPH_STROKE;
            for (x, y, horizontal_x, vertical_y) in [
                (left, top, left, top),
                (right - arm, top, right - thickness, top),
                (left, bottom - thickness, left, bottom - arm),
                (
                    right - arm,
                    bottom - thickness,
                    right - thickness,
                    bottom - arm,
                ),
            ] {
                context.fill_rect(Rect::new(Point::new(x, y), size(arm, thickness)), color);
                context.fill_rect(
                    Rect::new(Point::new(horizontal_x, vertical_y), size(thickness, arm)),
                    color,
                );
            }
        }
        WindowControl::Close => {
            for angle in [std::f32::consts::FRAC_PI_4, -std::f32::consts::FRAC_PI_4] {
                let transform = Affine::rotate(angle).then(Affine::translate(center.x, center.y));
                context.with_transform(transform, |context| {
                    context.fill_rect(
                        Rect::new(
                            Point::new(-half, -GLYPH_STROKE / 2.0),
                            size(GLYPH_SIZE, GLYPH_STROKE),
                        ),
                        color,
                    );
                });
            }
        }
    }
}

fn offset_rect(rect: Rect, offset: Point) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + offset.x, rect.origin.y + offset.y),
        rect.size,
    )
}

fn distance(first: Point, second: Point) -> f32 {
    let x = first.x - second.x;
    let y = first.y - second.y;
    (x * x + y * y).sqrt()
}

/// Converts a small widget count to `f32` without a lossy `usize` cast.
fn count_as_f32(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

fn size(width: f32, height: f32) -> Size {
    Size::new(width, height).expect("title bar sizes are non-negative and not NaN")
}

#[cfg(test)]
mod tests {
    use torn_core::{KeyCode, Modifiers, PointerButtons, PointerEvent, PointerId};
    use torn_ui::UiRuntime;

    use super::*;

    #[test]
    fn emits_commands_for_window_controls() {
        let mut runtime = UiRuntime::new(TitleBar::new("Torn"));
        runtime
            .layout(Constraints::tight(size(400.0, 38.0)).expect("valid constraints"))
            .expect("title bar layout succeeds");

        let width = 400.0;
        let total = 4.0 * DEFAULT_BUTTON_WIDTH;
        let start = width - 8.0 - total;
        for (index, expected) in [
            WindowCommand::Minimize,
            WindowCommand::ToggleMaximize,
            WindowCommand::ToggleFullscreen,
            WindowCommand::Close,
        ]
        .into_iter()
        .enumerate()
        {
            let x = start + count_as_f32(index) * DEFAULT_BUTTON_WIDTH + DEFAULT_BUTTON_WIDTH / 2.0;
            let _ = runtime.dispatch_event(&pointer(Point::new(x, 19.0), true));
            let _ = runtime.dispatch_event(&pointer(Point::new(x, 19.0), false));
            assert_eq!(runtime.take_window_commands(), vec![expected]);
        }
    }

    #[test]
    fn starts_a_drag_from_the_empty_title_area() {
        let mut runtime = UiRuntime::new(TitleBar::new("Torn"));
        runtime
            .layout(Constraints::tight(size(400.0, 38.0)).expect("valid constraints"))
            .expect("title bar layout succeeds");

        let _ = runtime.dispatch_event(&pointer(Point::new(20.0, 19.0), true));
        assert_eq!(
            runtime.take_window_commands(),
            vec![WindowCommand::StartDrag]
        );
    }

    #[test]
    fn double_click_toggles_maximize() {
        let mut runtime = UiRuntime::new(TitleBar::new("Torn"));
        runtime
            .layout(Constraints::tight(size(400.0, 38.0)).expect("valid constraints"))
            .expect("title bar layout succeeds");

        let _ = runtime.dispatch_event(&pointer(Point::new(20.0, 19.0), true));
        assert_eq!(
            runtime.take_window_commands(),
            vec![WindowCommand::StartDrag]
        );
        let _ = runtime.dispatch_event(&pointer(Point::new(20.0, 19.0), true));
        assert_eq!(
            runtime.take_window_commands(),
            vec![WindowCommand::ToggleMaximize]
        );
    }

    fn pointer(position: Point, down: bool) -> InputEvent {
        let event = PointerEvent {
            pointer_id: PointerId(1),
            position,
            button: Some(PointerButton::Primary),
            buttons: if down {
                PointerButtons::PRIMARY
            } else {
                PointerButtons::NONE
            },
            modifiers: Modifiers::NONE,
        };
        if down {
            InputEvent::PointerDown(event)
        } else {
            InputEvent::PointerUp(event)
        }
    }

    #[allow(dead_code)]
    fn key() -> InputEvent {
        InputEvent::KeyDown(torn_core::KeyEvent {
            key: torn_core::Key::Named(torn_core::NamedKey::Space),
            code: KeyCode::Unidentified,
            repeat: false,
            modifiers: Modifiers::NONE,
        })
    }
}
