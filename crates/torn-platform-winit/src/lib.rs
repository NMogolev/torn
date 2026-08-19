//! Cross-platform `winit` adapter for [`torn_platform::WindowApplication`].
//!
//! `winit` uses Win32 on Windows and X11 or Wayland on Linux. The adapter keeps
//! Torn's coordinate system logical and uses `pixels` to scale the logical RGBA
//! framebuffer to the native surface.

use std::{error::Error, fmt, sync::Arc};

use pixels::{Pixels, SurfaceTexture};
use torn_core::{
    InputEvent, Key, KeyCode, KeyEvent, Modifiers, NamedKey, Point, PointerButton, PointerButtons,
    PointerEvent, PointerId, Size, WheelDelta, WheelEvent,
};
use torn_platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions};
use torn_software::{PixelBuffer, SoftwareRenderWorker, SubmitError};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent as WinitWindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    keyboard::{Key as WinitKey, NamedKey as WinitNamedKey, PhysicalKey},
    window::{Window, WindowId},
};

/// Why [`run`] could not start or continue a native event loop.
#[derive(Debug)]
pub enum RunError {
    /// Creating or running the native event loop failed.
    EventLoop(winit::error::EventLoopError),
    /// Creating the native window failed.
    Window(winit::error::OsError),
    /// Initializing or presenting the pixel surface failed.
    Pixels(pixels::Error),
    /// Resizing a pixel texture failed.
    Texture(pixels::TextureError),
    /// Starting the software rendering worker failed.
    RenderWorker(std::io::Error),
    /// A requested logical size could not be represented as a Torn [`Size`].
    InvalidSize,
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(error) => write!(formatter, "native event loop failed: {error}"),
            Self::Window(error) => write!(formatter, "could not create native window: {error}"),
            Self::Pixels(error) => write!(formatter, "pixel surface failed: {error}"),
            Self::Texture(error) => write!(formatter, "pixel texture resize failed: {error}"),
            Self::RenderWorker(error) => {
                write!(formatter, "could not start render worker: {error}")
            }
            Self::InvalidSize => {
                formatter.write_str("native window reported an invalid logical size")
            }
        }
    }
}

impl Error for RunError {}

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
    let mut adapter = Adapter::new(Box::new(application), event_loop.create_proxy())?;
    event_loop
        .run_app(&mut adapter)
        .map_err(RunError::EventLoop)
}

/// A wake-up sent from the software render thread to the native event loop.
#[derive(Clone, Copy)]
enum UserEvent {
    RenderCompleted,
}

struct Adapter {
    application: Box<dyn WindowApplication>,
    options: WindowOptions,
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    logical_size: Option<Size>,
    render_worker: SoftwareRenderWorker,
    last_completed_frame: Option<PixelBuffer>,
    next_frame_id: u64,
    needs_render: bool,
    cursor: Point,
    buttons: PointerButtons,
    modifiers: Modifiers,
}

impl Adapter {
    fn new(
        application: Box<dyn WindowApplication>,
        event_proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self, RunError> {
        let options = application.window_options();
        let render_worker = SoftwareRenderWorker::spawn_with_result_notifier(move || {
            let _ = event_proxy.send_event(UserEvent::RenderCompleted);
        })
        .map_err(RunError::RenderWorker)?;
        Ok(Self {
            application,
            options,
            window: None,
            pixels: None,
            logical_size: None,
            render_worker,
            last_completed_frame: None,
            next_frame_id: 0,
            needs_render: false,
            cursor: Point::ZERO,
            buttons: PointerButtons::NONE,
            modifiers: Modifiers::NONE,
        })
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

    fn apply_action(&mut self, event_loop: &ActiveEventLoop, action: WindowAction) {
        match action {
            WindowAction::None => {}
            WindowAction::RequestRedraw => self.request_render(),
            WindowAction::Exit => event_loop.exit(),
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
        let surface = SurfaceTexture::new(physical_size.width, physical_size.height, window);
        self.pixels = Some(
            Pixels::new(physical_size.width, physical_size.height, surface)
                .map_err(RunError::Pixels)?,
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
        let pixels = self
            .pixels
            .as_mut()
            .expect("surface exists after window creation");
        pixels
            .resize_surface(physical_size.width, physical_size.height)
            .map_err(RunError::Texture)?;
        pixels
            .resize_buffer(physical_size.width, physical_size.height)
            .map_err(RunError::Texture)?;
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
        } else {
            event_loop.exit();
        }
    }

    fn dispatch(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let action = self.application.window_event(event);
        self.apply_action(event_loop, action);
    }

    fn submit_display_list(&mut self, _: Size) -> Result<(), RunError> {
        let frame_id = self.next_frame_id;
        self.next_frame_id = self.next_frame_id.wrapping_add(1);
        let window = self.window.as_ref().expect("window exists while rendering");
        let physical_size = window.inner_size();
        let scale_factor = to_scale_factor(window.scale_factor())?;
        let display_list = self.application.redraw();
        match self.render_worker.try_submit_with_scale_factor(
            frame_id,
            display_list,
            physical_size.width,
            physical_size.height,
            scale_factor,
        ) {
            Ok(()) => {}
            Err(SubmitError::QueueFull) => self.needs_render = true,
            Err(SubmitError::Stopped) => self.needs_render = false,
        }
        Ok(())
    }

    fn receive_render_results(&mut self) -> bool {
        let mut received = false;
        loop {
            match self.render_worker.try_receive() {
                Ok(Some(result)) => {
                    received = true;
                    if let Ok(frame) = result.into_result() {
                        self.last_completed_frame = Some(frame);
                    }
                }
                Ok(None) => return received,
                Err(_) => return false,
            }
        }
    }

    fn copy_last_completed_frame(frame: Option<&PixelBuffer>, destination: &mut [u8]) {
        destination.fill(0);
        let Some(frame) = frame else {
            return;
        };
        let expected_byte_count =
            usize::try_from(u64::from(frame.width()) * u64::from(frame.height()) * 4)
                .unwrap_or(usize::MAX);
        if destination.len() != expected_byte_count {
            return;
        }
        for (destination, source) in destination.chunks_exact_mut(4).zip(frame.pixels()) {
            destination.copy_from_slice(&[source.red, source.green, source.blue, source.alpha]);
        }
    }
}

impl ApplicationHandler<UserEvent> for Adapter {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(&self.options.title)
            .with_inner_size(LogicalSize::new(
                f64::from(self.options.size.width()),
                f64::from(self.options.size.height()),
            ));
        let Ok(window) = event_loop.create_window(attributes) else {
            event_loop.exit();
            return;
        };
        self.window = Some(Arc::new(window));
        let window = self.window.as_ref().expect("window was stored");
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
        let scale_factor = self.window.as_ref().expect("window exists").scale_factor();
        match event {
            WinitWindowEvent::CloseRequested => {
                self.dispatch(event_loop, WindowEvent::CloseRequested);
            }
            WinitWindowEvent::Resized(physical_size) => {
                self.handle_resize(event_loop, physical_size, scale_factor);
            }
            WinitWindowEvent::ScaleFactorChanged { .. } => {
                let physical_size = self.window.as_ref().expect("window exists").inner_size();
                self.handle_resize(event_loop, physical_size, scale_factor);
            }
            WinitWindowEvent::CursorMoved { position, .. } => {
                self.cursor = to_logical_point(position, scale_factor);
                self.dispatch(
                    event_loop,
                    WindowEvent::Input(pointer_input(
                        self.cursor,
                        None,
                        &self.buttons,
                        self.modifiers,
                        false,
                    )),
                );
            }
            WinitWindowEvent::MouseInput { state, button, .. } => {
                let button = pointer_button(button);
                update_pointer_buttons(&mut self.buttons, button, state);
                self.dispatch(
                    event_loop,
                    WindowEvent::Input(pointer_input(
                        self.cursor,
                        Some(button),
                        &self.buttons,
                        self.modifiers,
                        state == ElementState::Pressed,
                    )),
                );
            }
            WinitWindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines(Point::new(x, y)),
                    MouseScrollDelta::PixelDelta(position) => {
                        WheelDelta::Pixels(to_logical_point(position, scale_factor))
                    }
                };
                self.dispatch(
                    event_loop,
                    WindowEvent::Input(InputEvent::Wheel(WheelEvent {
                        position: self.cursor,
                        delta,
                        modifiers: self.modifiers,
                    })),
                );
            }
            WinitWindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers_from_winit(modifiers.state());
            }
            WinitWindowEvent::KeyboardInput { event, .. } => {
                let input = if event.state == ElementState::Pressed {
                    InputEvent::KeyDown(key_event(&event, self.modifiers))
                } else {
                    InputEvent::KeyUp(key_event(&event, self.modifiers))
                };
                self.dispatch(event_loop, WindowEvent::Input(input));
            }
            WinitWindowEvent::Ime(Ime::Commit(text)) => {
                self.dispatch(event_loop, WindowEvent::Input(InputEvent::TextInput(text)));
            }
            WinitWindowEvent::RedrawRequested => {
                let Some(size) = self.logical_size else {
                    return;
                };
                if self.needs_render {
                    self.needs_render = false;
                    if self.submit_display_list(size).is_err() {
                        event_loop.exit();
                        return;
                    }
                }
                let last_completed_frame = self.last_completed_frame.as_ref();
                let Some(pixels) = &mut self.pixels else {
                    return;
                };
                Self::copy_last_completed_frame(last_completed_frame, pixels.frame_mut());
                if pixels.render().is_err() {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::RenderCompleted if self.receive_render_results() => self.request_redraw(),
            UserEvent::RenderCompleted => {}
        }
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
