//! Cross-platform `winit` adapter for [`torn_platform::WindowApplication`].
//!
//! `winit` uses Win32 on Windows and X11 or Wayland on Linux. The adapter keeps
//! Torn's coordinate system logical and renders display lists directly with
//! `wgpu` to the native surface.

use std::{error::Error, fmt, sync::Arc, time::Instant};

use torn_core::{
    Clipboard, InputEvent, Key, KeyCode, KeyEvent, Modifiers, NamedKey, Point, PointerButton,
    PointerButtons, PointerEvent, PointerId, Size, TextPreedit, WheelDelta, WheelEvent,
};
use torn_platform::{
    WindowAction, WindowApplication, WindowEvent, WindowOptions, WindowResizeDirection,
};
use torn_wgpu::{GpuError, GpuRenderer, RenderStatus};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::{
        ElementState, Ime, MouseButton, MouseScrollDelta, StartCause,
        WindowEvent as WinitWindowEvent,
    },
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key as WinitKey, NamedKey as WinitNamedKey, PhysicalKey},
    window::{Fullscreen, ResizeDirection, Window, WindowId},
};

/// Why [`run`] could not start or continue a native event loop.
#[derive(Debug)]
pub enum RunError {
    /// Creating or running the native event loop failed.
    EventLoop(winit::error::EventLoopError),
    /// Creating the native window failed.
    Window(winit::error::OsError),
    /// Initializing or presenting the GPU surface failed.
    Gpu(GpuError),
    /// A requested logical size could not be represented as a Torn [`Size`].
    InvalidSize,
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(error) => write!(formatter, "native event loop failed: {error}"),
            Self::Window(error) => write!(formatter, "could not create native window: {error}"),
            Self::Gpu(error) => write!(formatter, "GPU renderer failed: {error}"),
            Self::InvalidSize => {
                formatter.write_str("native window reported an invalid logical size")
            }
        }
    }
}

impl Error for RunError {}

/// A clipboard backed by the operating system clipboard.
///
/// Applications can install this adapter with
/// [`torn_ui::UiRuntime::set_clipboard`] to give editable widgets real
/// cut, copy, and paste behavior. Construction succeeds even when the platform
/// clipboard is unavailable; reads then return `None` and writes are ignored.
pub struct ArboardClipboard {
    clipboard: Option<arboard::Clipboard>,
}

impl ArboardClipboard {
    /// Connects to the operating system clipboard.
    #[must_use]
    pub fn new() -> Self {
        Self {
            clipboard: arboard::Clipboard::new().ok(),
        }
    }

    /// Returns whether the operating system clipboard is available.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.clipboard.is_some()
    }
}

impl Default for ArboardClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard for ArboardClipboard {
    fn read_text(&mut self) -> Option<String> {
        self.clipboard.as_mut()?.get_text().ok()
    }

    fn write_text(&mut self, text: &str) {
        if let Some(clipboard) = self.clipboard.as_mut() {
            let _ = clipboard.set_text(text.to_owned());
        }
    }
}

/// Runs `application` in a native event loop.
///
/// The returned function is portable across Windows and Linux environments with
/// an available Win32, X11, or Wayland display server.
///
/// # Errors
///
/// Returns an error when the event loop cannot be created. Later platform and
/// presentation failures close the event loop because `winit` callbacks cannot
/// return errors directly.
pub fn run(application: impl WindowApplication + 'static) -> Result<(), RunError> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(RunError::EventLoop)?;
    let proxy = event_loop.create_proxy();
    let mut adapter = Adapter::new(Box::new(application), proxy);
    event_loop
        .run_app(&mut adapter)
        .map_err(RunError::EventLoop)
}

/// A custom event delivered through the native event loop.
#[derive(Debug)]
enum UserEvent {
    /// An accessibility request from the platform adapter.
    Accessibility(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Accessibility(event)
    }
}

struct Adapter {
    application: Box<dyn WindowApplication>,
    options: WindowOptions,
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    accessibility: Option<accesskit_winit::Adapter>,
    event_loop_proxy: EventLoopProxy<UserEvent>,
    logical_size: Option<Size>,
    needs_render: bool,
    maximized: bool,
    fullscreen: bool,
    cursor: Point,
    buttons: PointerButtons,
    modifiers: Modifiers,
    animation_deadline: Option<Instant>,
}

impl Adapter {
    fn new(
        application: Box<dyn WindowApplication>,
        event_loop_proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        let options = application.window_options();
        let maximized = options.maximized;
        let fullscreen = options.fullscreen;
        Self {
            application,
            options,
            window: None,
            renderer: None,
            accessibility: None,
            event_loop_proxy,
            logical_size: None,
            needs_render: false,
            maximized,
            fullscreen,
            cursor: Point::ZERO,
            buttons: PointerButtons::NONE,
            modifiers: Modifiers::NONE,
            animation_deadline: None,
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn request_render(&mut self) {
        self.needs_render = true;
        self.request_redraw();
    }

    fn schedule_animation(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            return;
        }
        let Some(delay) = self.application.next_frame_in() else {
            self.animation_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        let deadline = Instant::now() + delay;
        self.animation_deadline = Some(deadline);
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
    }

    fn apply_action(&mut self, event_loop: &ActiveEventLoop, action: WindowAction) {
        match action {
            WindowAction::None => {}
            WindowAction::RequestRedraw => self.request_render(),
            WindowAction::Exit => event_loop.exit(),
            WindowAction::Minimize => {
                if let Some(window) = &self.window {
                    window.set_minimized(true);
                }
            }
            WindowAction::ToggleMaximize => {
                if let Some(window) = &self.window {
                    window.set_maximized(!window.is_maximized());
                }
            }
            WindowAction::ToggleFullscreen => {
                if let Some(window) = &self.window {
                    self.fullscreen = window.fullscreen().is_none();
                    window.set_fullscreen(self.fullscreen.then_some(Fullscreen::Borderless(None)));
                }
            }
            WindowAction::StartDrag => {
                if let Some(window) = &self.window {
                    let _ = window.drag_window();
                }
            }
            WindowAction::StartResize(direction) => {
                if let Some(window) = &self.window {
                    let _ = window.drag_resize_window(resize_direction(direction));
                }
            }
        }
    }

    fn sync_window_state(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = &self.window else {
            return;
        };
        let maximized = window.is_maximized();
        let fullscreen = window.fullscreen().is_some();
        let maximized_changed = maximized != self.maximized;
        let fullscreen_changed = fullscreen != self.fullscreen;
        self.maximized = maximized;
        self.fullscreen = fullscreen;
        if maximized_changed {
            self.dispatch(event_loop, WindowEvent::Maximized(maximized));
        }
        if fullscreen_changed {
            self.dispatch(event_loop, WindowEvent::Fullscreen(fullscreen));
        }
    }

    fn create_surface(
        &mut self,
        physical_size: PhysicalSize<u32>,
        scale_factor: f64,
    ) -> Result<(), RunError> {
        let logical_size = logical_size(physical_size, scale_factor)?;
        let window = Arc::clone(
            self.window
                .as_ref()
                .expect("window exists before surface creation"),
        );
        self.renderer = Some(
            GpuRenderer::new(window, physical_size.width, physical_size.height)
                .map_err(RunError::Gpu)?,
        );
        self.logical_size = Some(logical_size);
        Ok(())
    }

    fn resize_surface(
        &mut self,
        physical_size: PhysicalSize<u32>,
        scale_factor: f64,
    ) -> Result<(), RunError> {
        if physical_size.width == 0 || physical_size.height == 0 {
            return Ok(());
        }
        let logical_size = logical_size(physical_size, scale_factor)?;
        let renderer = self
            .renderer
            .as_mut()
            .expect("surface exists after window creation");
        renderer
            .resize(physical_size.width, physical_size.height)
            .map_err(RunError::Gpu)?;
        self.logical_size = Some(logical_size);
        Ok(())
    }

    fn handle_resize(
        &mut self,
        event_loop: &ActiveEventLoop,
        physical_size: PhysicalSize<u32>,
        scale_factor: f64,
    ) {
        if self.resize_surface(physical_size, scale_factor).is_ok() {
            if let Some(size) = self.logical_size {
                self.dispatch(event_loop, WindowEvent::Resized(size));
            }
            self.sync_window_state(event_loop);
        } else {
            event_loop.exit();
        }
    }

    fn dispatch(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let refresh_accessibility = should_refresh_accessibility(&event);
        let action = self.application.window_event(event);
        self.apply_action(event_loop, action);
        if refresh_accessibility {
            self.refresh_accessibility();
        }
        self.schedule_animation(event_loop);
    }

    fn refresh_accessibility(&mut self) {
        let Some(tree) = self.application.accessibility_tree() else {
            return;
        };
        if let Some(adapter) = self.accessibility.as_mut() {
            adapter.update_if_active(|| tree);
        }
    }

    fn handle_accessibility_event(&mut self, event: accesskit_winit::Event) {
        match event.window_event {
            accesskit_winit::WindowEvent::InitialTreeRequested => self.refresh_accessibility(),
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                if self.application.accessibility_action(&request) {
                    self.refresh_accessibility();
                    self.request_render();
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }

    fn handle_input_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        event: WinitWindowEvent,
        scale_factor: f64,
    ) -> bool {
        let input = match event {
            WinitWindowEvent::CursorMoved { position, .. } => {
                self.cursor = to_logical_point(position, scale_factor);
                pointer_input(self.cursor, None, &self.buttons, self.modifiers, false)
            }
            WinitWindowEvent::CursorLeft { .. } => InputEvent::PointerLeave(PointerEvent {
                pointer_id: PointerId(0),
                position: self.cursor,
                button: None,
                buttons: self.buttons.clone(),
                modifiers: self.modifiers,
            }),
            WinitWindowEvent::MouseInput { state, button, .. } => {
                let button = pointer_button(button);
                update_pointer_buttons(&mut self.buttons, button, state);
                pointer_input(
                    self.cursor,
                    Some(button),
                    &self.buttons,
                    self.modifiers,
                    state == ElementState::Pressed,
                )
            }
            WinitWindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines(Point::new(x, y)),
                    MouseScrollDelta::PixelDelta(position) => {
                        WheelDelta::Pixels(to_logical_point(position, scale_factor))
                    }
                };
                InputEvent::Wheel(WheelEvent {
                    position: self.cursor,
                    delta,
                    modifiers: self.modifiers,
                })
            }
            WinitWindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers_from_winit(modifiers.state());
                return true;
            }
            WinitWindowEvent::Focused(focused) => {
                if !focused {
                    self.buttons = PointerButtons::NONE;
                    self.modifiers = Modifiers::NONE;
                }
                InputEvent::WindowFocusChanged(focused)
            }
            WinitWindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    InputEvent::KeyDown(key_event(&event, self.modifiers))
                } else {
                    InputEvent::KeyUp(key_event(&event, self.modifiers))
                }
            }
            WinitWindowEvent::Ime(ime) => match ime_input(ime) {
                Some(input) => input,
                None => return false,
            },
            _ => return false,
        };
        self.dispatch(event_loop, WindowEvent::Input(input));
        true
    }

    fn handle_redraw(&mut self, event_loop: &ActiveEventLoop, scale_factor: f64) {
        if self.logical_size.is_none() {
            return;
        }
        self.dispatch(event_loop, WindowEvent::RedrawRequested);
        self.needs_render = false;
        match self.render_display_list() {
            Ok(RenderStatus::Presented | RenderStatus::Skipped) => {}
            Ok(RenderStatus::Suboptimal | RenderStatus::Reconfigure) => {
                let physical_size = self.window.as_ref().expect("window exists").inner_size();
                if self.resize_surface(physical_size, scale_factor).is_ok() {
                    self.request_render();
                } else {
                    event_loop.exit();
                }
            }
            Err(_) => event_loop.exit(),
        }
        self.schedule_animation(event_loop);
    }

    fn render_display_list(&mut self) -> Result<RenderStatus, RunError> {
        let window = self.window.as_ref().expect("window exists while rendering");
        let scale_factor = to_scale_factor(window.scale_factor())?;
        let display_list = self.application.redraw();
        self.renderer
            .as_ref()
            .expect("surface exists while rendering")
            .render(&display_list, scale_factor)
            .map_err(|_| RunError::InvalidSize)
    }
}

impl ApplicationHandler<UserEvent> for Adapter {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. })
            && self.animation_deadline.take().is_some()
        {
            event_loop.set_control_flow(ControlFlow::Wait);
            self.request_redraw();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        let UserEvent::Accessibility(event) = event;
        self.handle_accessibility_event(event);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attributes = Window::default_attributes()
            .with_title(&self.options.title)
            .with_inner_size(LogicalSize::new(
                f64::from(self.options.size.width()),
                f64::from(self.options.size.height()),
            ))
            .with_decorations(self.options.decorations)
            .with_resizable(self.options.resizable)
            .with_maximized(self.options.maximized)
            .with_visible(false)
            .with_fullscreen(
                self.options
                    .fullscreen
                    .then_some(Fullscreen::Borderless(None)),
            );
        if let Some(minimum) = self.options.min_size {
            attributes = attributes.with_min_inner_size(LogicalSize::new(
                f64::from(minimum.width()),
                f64::from(minimum.height()),
            ));
        }
        #[cfg(target_os = "windows")]
        if !self.options.decorations {
            use winit::platform::windows::WindowAttributesExtWindows;

            attributes = attributes.with_undecorated_shadow(true);
        }
        let Ok(window) = event_loop.create_window(attributes) else {
            event_loop.exit();
            return;
        };
        self.window = Some(Arc::new(window));
        let window = self.window.as_ref().expect("window was stored");
        {
            let proxy = self.event_loop_proxy.clone();
            self.accessibility = Some(accesskit_winit::Adapter::with_event_loop_proxy(
                event_loop, window, proxy,
            ));
        }
        window.set_visible(true);
        self.maximized = window.is_maximized();
        self.fullscreen = window.fullscreen().is_some();
        let physical_size = window.inner_size();
        let scale_factor = window.scale_factor();
        if self.create_surface(physical_size, scale_factor).is_err() {
            event_loop.exit();
            return;
        }
        if let Some(size) = self.logical_size {
            self.dispatch(event_loop, WindowEvent::Resized(size));
        }
        self.request_render();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WinitWindowEvent,
    ) {
        if self.window.as_ref().map(|window| window.id()) != Some(window_id) {
            return;
        }
        if let (Some(accessibility), Some(window)) =
            (self.accessibility.as_mut(), self.window.as_ref())
        {
            accessibility.process_event(window, &event);
        }
        let scale_factor = self.window.as_ref().expect("window exists").scale_factor();
        match event {
            WinitWindowEvent::CloseRequested => {
                self.dispatch(event_loop, WindowEvent::CloseRequested);
            }
            WinitWindowEvent::Resized(physical_size) => {
                self.handle_resize(event_loop, physical_size, scale_factor);
            }
            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let physical_size = self.window.as_ref().expect("window exists").inner_size();
                self.handle_resize(event_loop, physical_size, scale_factor);
            }
            WinitWindowEvent::RedrawRequested => {
                self.handle_redraw(event_loop, scale_factor);
            }
            event => {
                let _ = self.handle_input_event(event_loop, event, scale_factor);
            }
        }
    }
}

fn should_refresh_accessibility(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::Input(InputEvent::PointerMove(_) | InputEvent::PointerLeave(_))
        | WindowEvent::Maximized(_)
        | WindowEvent::Fullscreen(_)
        | WindowEvent::RedrawRequested
        | WindowEvent::CloseRequested => false,
        WindowEvent::Resized(_) | WindowEvent::Input(_) => true,
    }
}

fn logical_size(physical: PhysicalSize<u32>, scale_factor: f64) -> Result<Size, RunError> {
    let logical: LogicalSize<f64> = physical.to_logical(scale_factor);
    Size::new(
        to_logical_coordinate(logical.width),
        to_logical_coordinate(logical.height),
    )
    .map_err(|_| RunError::InvalidSize)
}

fn to_logical_point(position: winit::dpi::PhysicalPosition<f64>, scale_factor: f64) -> Point {
    let logical: LogicalPosition<f64> = position.to_logical(scale_factor);
    Point::new(
        to_logical_coordinate(logical.x),
        to_logical_coordinate(logical.y),
    )
}

fn to_logical_coordinate(value: f64) -> f32 {
    let value = value.clamp(f64::from(f32::MIN), f64::from(f32::MAX));
    // The clamp limits the value to the finite f32 range.
    #[allow(clippy::cast_possible_truncation)]
    {
        value as f32
    }
}

fn to_scale_factor(value: f64) -> Result<f32, RunError> {
    if !value.is_finite() || value <= 0.0 || value > f64::from(f32::MAX) {
        return Err(RunError::InvalidSize);
    }
    #[allow(clippy::cast_possible_truncation)]
    {
        Ok(value as f32)
    }
}

fn pointer_input(
    position: Point,
    button: Option<PointerButton>,
    buttons: &PointerButtons,
    modifiers: Modifiers,
    pressed: bool,
) -> InputEvent {
    let event = PointerEvent {
        pointer_id: PointerId(0),
        position,
        button,
        buttons: buttons.clone(),
        modifiers,
    };
    if pressed {
        InputEvent::PointerDown(event)
    } else if button.is_some() {
        InputEvent::PointerUp(event)
    } else {
        InputEvent::PointerMove(event)
    }
}

fn ime_input(ime: Ime) -> Option<InputEvent> {
    Some(match ime {
        Ime::Commit(text) => InputEvent::TextInput(text),
        Ime::Preedit(text, cursor) => InputEvent::TextPreedit(TextPreedit { text, cursor }),
        Ime::Disabled => InputEvent::TextPreedit(TextPreedit {
            text: String::new(),
            cursor: None,
        }),
        Ime::Enabled => return None,
    })
}

fn resize_direction(direction: WindowResizeDirection) -> ResizeDirection {
    match direction {
        WindowResizeDirection::North => ResizeDirection::North,
        WindowResizeDirection::South => ResizeDirection::South,
        WindowResizeDirection::West => ResizeDirection::West,
        WindowResizeDirection::East => ResizeDirection::East,
        WindowResizeDirection::NorthWest => ResizeDirection::NorthWest,
        WindowResizeDirection::NorthEast => ResizeDirection::NorthEast,
        WindowResizeDirection::SouthWest => ResizeDirection::SouthWest,
        WindowResizeDirection::SouthEast => ResizeDirection::SouthEast,
    }
}

fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Middle => PointerButton::Auxiliary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Back => PointerButton::Other(4),
        MouseButton::Forward => PointerButton::Other(5),
        MouseButton::Other(value) => PointerButton::Other(value),
    }
}

fn update_pointer_buttons(
    buttons: &mut PointerButtons,
    button: PointerButton,
    state: ElementState,
) {
    if state == ElementState::Pressed {
        buttons.insert(button);
    } else {
        buttons.remove(button);
    }
}

fn modifiers_from_winit(modifiers: winit::keyboard::ModifiersState) -> Modifiers {
    let mut result = Modifiers::NONE;
    if modifiers.shift_key() {
        result |= Modifiers::SHIFT;
    }
    if modifiers.control_key() {
        result |= Modifiers::CONTROL;
    }
    if modifiers.alt_key() {
        result |= Modifiers::ALT;
    }
    if modifiers.super_key() {
        result |= Modifiers::META;
    }
    result
}

fn key_event(event: &winit::event::KeyEvent, modifiers: Modifiers) -> KeyEvent {
    KeyEvent {
        key: key_from_winit(&event.logical_key),
        code: match event.physical_key {
            PhysicalKey::Code(code) => KeyCode::Platform(code as u32),
            PhysicalKey::Unidentified(_) => KeyCode::Unidentified,
        },
        repeat: event.repeat,
        modifiers,
    }
}

fn key_from_winit(key: &WinitKey) -> Key {
    match key {
        WinitKey::Character(character) => Key::Character(character.to_string()),
        WinitKey::Named(named) => named_key(*named).map_or(Key::Unidentified, Key::Named),
        WinitKey::Unidentified(_) | WinitKey::Dead(_) => Key::Unidentified,
    }
}

fn named_key(key: WinitNamedKey) -> Option<NamedKey> {
    Some(match key {
        WinitNamedKey::Backspace => NamedKey::Backspace,
        WinitNamedKey::Enter => NamedKey::Enter,
        WinitNamedKey::Escape => NamedKey::Escape,
        WinitNamedKey::Tab => NamedKey::Tab,
        WinitNamedKey::Space => NamedKey::Space,
        WinitNamedKey::ArrowLeft => NamedKey::ArrowLeft,
        WinitNamedKey::ArrowRight => NamedKey::ArrowRight,
        WinitNamedKey::ArrowUp => NamedKey::ArrowUp,
        WinitNamedKey::ArrowDown => NamedKey::ArrowDown,
        WinitNamedKey::Home => NamedKey::Home,
        WinitNamedKey::End => NamedKey::End,
        WinitNamedKey::PageUp => NamedKey::PageUp,
        WinitNamedKey::PageDown => NamedKey::PageDown,
        WinitNamedKey::Delete => NamedKey::Delete,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{pointer_button, pointer_input, update_pointer_buttons};
    use torn_core::{InputEvent, Modifiers, Point, PointerButton, PointerButtons};
    use winit::event::{ElementState, MouseButton};

    #[test]
    fn pointer_button_state_is_preserved_across_move_and_release() {
        let mut buttons = PointerButtons::NONE;
        let primary = pointer_button(MouseButton::Left);
        let back = pointer_button(MouseButton::Back);
        update_pointer_buttons(&mut buttons, primary, ElementState::Pressed);
        let InputEvent::PointerDown(event) =
            pointer_input(Point::ZERO, Some(primary), &buttons, Modifiers::NONE, true)
        else {
            panic!("expected pointer down");
        };
        assert!(event.buttons.contains_button(PointerButton::Primary));

        update_pointer_buttons(&mut buttons, back, ElementState::Pressed);

        let InputEvent::PointerMove(event) =
            pointer_input(Point::ZERO, None, &buttons, Modifiers::NONE, false)
        else {
            panic!("expected pointer move");
        };
        assert!(event.buttons.contains_button(PointerButton::Primary));
        assert!(event.buttons.contains_button(PointerButton::Other(4)));

        update_pointer_buttons(&mut buttons, primary, ElementState::Released);
        let InputEvent::PointerUp(event) =
            pointer_input(Point::ZERO, Some(primary), &buttons, Modifiers::NONE, false)
        else {
            panic!("expected pointer up");
        };
        assert!(!event.buttons.contains_button(PointerButton::Primary));
        assert!(event.buttons.contains_button(PointerButton::Other(4)));
    }
}
