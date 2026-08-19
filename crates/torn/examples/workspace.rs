//! An interactive desktop-workspace vertical slice.
//!
//! Run with `cargo run -p torn --example workspace`. The Save button writes a
//! readable `workspace-layout.json` beside the workspace `Cargo.toml`; Restore
//! validates and applies that file without serializing live widgets.

use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use torn::{
    Button, ChildLayout, Color, Constraints, DockArea, DocumentId, LayoutContext, LayoutNode,
    LayoutResult, PanelId, Point, Rect, Size, UiEnvironment, UiRuntime, Widget, WorkspaceLayout,
    platform::{WindowAction, WindowApplication, WindowEvent, WindowOptions},
    render::{DisplayList, FontdueTextShaper, PaintContext, TextStyle},
};

const TOOLBAR_HEIGHT: f32 = 44.0;
const TOOLBAR_BUTTON_WIDTH: f32 = 130.0;

fn main() -> Result<(), torn_platform_winit::RunError> {
    torn_platform_winit::run(WorkspaceDemo::new())
}

struct WorkspaceDemo {
    runtime: UiRuntime,
    size: Size,
    layout: Rc<RefCell<WorkspaceLayout>>,
    layout_path: PathBuf,
}

impl WorkspaceDemo {
    fn new() -> Self {
        let size = Size::new(1_120.0, 720.0).expect("initial window size is valid");
        let layout_path = PathBuf::from("workspace-layout.json");
        let layout = Rc::new(RefCell::new(
            load_layout(&layout_path).unwrap_or_else(default_layout),
        ));
        let mut runtime = UiRuntime::new(WorkspaceShell);
        let root = runtime.root();

        let mut save = Button::new();
        {
            let layout = Rc::clone(&layout);
            let layout_path = layout_path.clone();
            save.set_on_click(move || save_layout(&layout.borrow(), &layout_path));
        }
        let save = runtime.append_child(root, save).expect("root exists");
        runtime
            .append_child(save, label("Сохранить", 13.0, Color::WHITE))
            .expect("save button exists");

        let mut restore = Button::new();
        {
            let layout = Rc::clone(&layout);
            let layout_path = layout_path.clone();
            restore.set_on_click(move || {
                if let Some(restored) = load_layout(&layout_path) {
                    *layout.borrow_mut() = restored;
                    println!("Рабочая область восстановлена из {}", layout_path.display());
                }
            });
        }
        let restore = runtime.append_child(root, restore).expect("root exists");
        runtime
            .append_child(restore, label("Восстановить", 13.0, Color::WHITE))
            .expect("restore button exists");

        let mut dock_area = DockArea::new(Rc::clone(&layout));
        for panel in ["project", "inspector"] {
            dock_area
                .register_panel(PanelId::from(panel))
                .expect("every dock item is registered once");
        }
        for document in ["welcome", "scene", "notes"] {
            dock_area
                .register_document(DocumentId::from(document))
                .expect("every dock item is registered once");
        }
        let dock = runtime.append_child(root, dock_area).expect("root exists");
        append_panel(
            &mut runtime,
            dock,
            "Проект",
            "assets\nscenes\nmaterials",
            Color::rgba8(44, 66, 89, 255),
        );
        append_panel(
            &mut runtime,
            dock,
            "Инспектор",
            "Transform\nPosition  12, 48\nScale       1.0",
            Color::rgba8(73, 55, 88, 255),
        );
        append_panel(
            &mut runtime,
            dock,
            "Добро пожаловать",
            "Torn workspace\n\nПереключайте вкладки и тяните разделители.",
            Color::rgba8(45, 81, 70, 255),
        );
        append_panel(
            &mut runtime,
            dock,
            "Сцена",
            "Scene.graph\n\nCamera\nLight\nMesh",
            Color::rgba8(70, 75, 50, 255),
        );
        append_panel(
            &mut runtime,
            dock,
            "Заметки",
            "Этап 4: vertical slice\n\nСостояние workspace сохраняется в JSON.",
            Color::rgba8(75, 55, 45, 255),
        );

        let mut app = Self {
            runtime,
            size,
            layout,
            layout_path,
        };
        app.layout(size);
        app
    }

    fn layout(&mut self, size: Size) {
        self.size = size;
        if let Ok(constraints) = Constraints::tight(size) {
            let _ = self.runtime.layout(constraints);
        }
    }
}

impl Drop for WorkspaceDemo {
    fn drop(&mut self) {
        save_layout(&self.layout.borrow(), &self.layout_path);
    }
}

impl WindowApplication for WorkspaceDemo {
    fn window_options(&self) -> WindowOptions {
        WindowOptions::new("Torn — Workspace", self.size)
    }

    fn window_event(&mut self, event: WindowEvent) -> WindowAction {
        match event {
            WindowEvent::CloseRequested => WindowAction::Exit,
            WindowEvent::Resized(size) => {
                self.layout(size);
                WindowAction::RequestRedraw
            }
            WindowEvent::Input(event) => {
                let handled = self.runtime.dispatch_event(&event).is_handled();
                if handled {
                    // Tab activation and splitter dragging alter the model, so its
                    // retained projection needs a fresh layout before painting.
                    self.layout(self.size);
                }
                if self.runtime.take_redraw_request() || handled {
                    WindowAction::RequestRedraw
                } else {
                    WindowAction::None
                }
            }
            WindowEvent::RedrawRequested => WindowAction::None,
        }
    }

    fn redraw(&mut self) -> DisplayList {
        let mut display_list = DisplayList::new();
        let _ = self
            .runtime
            .paint(&mut PaintContext::new(&mut display_list));
        display_list
    }
}

struct WorkspaceShell;

impl Widget for WorkspaceShell {
    fn layout(
        &mut self,
        context: &mut LayoutContext<'_>,
        constraints: Constraints,
    ) -> LayoutResult {
        let size = available_size(constraints);
        let toolbar_height = size.height().min(TOOLBAR_HEIGHT);
        let save_width = size.width().min(TOOLBAR_BUTTON_WIDTH);
        let restore_width = (size.width() - save_width).min(TOOLBAR_BUTTON_WIDTH);
        let (save, _) = context
            .layout_child(0, tight(save_width, toolbar_height))
            .expect("save button is the first shell child");
        let (restore, _) = context
            .layout_child(1, tight(restore_width, toolbar_height))
            .expect("restore button is the second shell child");
        let (workspace, _) = context
            .layout_child(
                2,
                tight(size.width(), (size.height() - toolbar_height).max(0.0)),
            )
            .expect("dock area is the third shell child");

        LayoutResult::with_children(
            size,
            vec![
                ChildLayout::new(save, Point::ZERO),
                ChildLayout::new(restore, Point::new(save_width, 0.0)),
                ChildLayout::new(workspace, Point::new(0.0, toolbar_height)),
            ],
        )
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, Color::rgba8(25, 28, 34, 255));
        context.fill_rect(
            Rect::new(
                bounds.origin,
                Size::new(
                    bounds.size.width(),
                    TOOLBAR_HEIGHT.min(bounds.size.height()),
                )
                .expect("valid toolbar size"),
            ),
            Color::rgba8(37, 42, 50, 255),
        );
    }
}

struct Panel {
    title: &'static str,
    body: &'static str,
    color: Color,
}

impl Widget for Panel {
    fn layout(&mut self, _: &mut LayoutContext<'_>, constraints: Constraints) -> LayoutResult {
        LayoutResult::new(available_size(constraints))
    }

    fn paint(&self, context: &mut PaintContext<'_>, _: &UiEnvironment, bounds: Rect) {
        context.fill_rect(bounds, self.color);
        let shaper = FontdueTextShaper::ubuntu_light();
        context.draw_text(
            shaper.layout(self.title, &TextStyle::new(19.0, Color::WHITE), None),
            Point::new(bounds.origin.x + 18.0, bounds.origin.y + 18.0),
        );
        context.draw_text(
            shaper.layout(
                self.body,
                &TextStyle::new(14.0, Color::rgba8(230, 235, 240, 255)),
                None,
            ),
            Point::new(bounds.origin.x + 18.0, bounds.origin.y + 58.0),
        );
    }
}

fn default_layout() -> WorkspaceLayout {
    WorkspaceLayout::new(LayoutNode::split(
        torn::DockAxis::Horizontal,
        0.22,
        LayoutNode::Panel {
            id: PanelId::from("project"),
        },
        LayoutNode::split(
            torn::DockAxis::Horizontal,
            0.72,
            LayoutNode::documents(vec![
                DocumentId::from("welcome"),
                DocumentId::from("scene"),
                DocumentId::from("notes"),
            ]),
            LayoutNode::Panel {
                id: PanelId::from("inspector"),
            },
        ),
    ))
    .expect("built-in layout is valid")
}

fn load_layout(path: &PathBuf) -> Option<WorkspaceLayout> {
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!("Не удалось прочитать {}: {error}", path.display());
            return None;
        }
    };
    match WorkspaceLayout::from_json(&json) {
        Ok(layout) => Some(layout),
        Err(error) => {
            eprintln!("Не удалось восстановить {}: {error}", path.display());
            None
        }
    }
}

fn save_layout(layout: &WorkspaceLayout, path: &PathBuf) {
    match layout.to_json() {
        Ok(json) => match fs::write(path, json) {
            Ok(()) => println!("Рабочая область сохранена в {}", path.display()),
            Err(error) => eprintln!("Не удалось сохранить {}: {error}", path.display()),
        },
        Err(error) => eprintln!("Не удалось подготовить JSON workspace: {error}"),
    }
}

fn append_panel(
    runtime: &mut UiRuntime,
    parent: torn::WidgetId,
    title: &'static str,
    body: &'static str,
    color: Color,
) {
    runtime
        .append_child(parent, Panel { title, body, color })
        .expect("dock area exists");
}

fn label(text: &str, size: f32, color: Color) -> torn::Text {
    torn::Text::new(FontdueTextShaper::ubuntu_light().layout(
        text,
        &TextStyle::new(size, color),
        None,
    ))
}

fn available_size(constraints: Constraints) -> Size {
    let max = constraints.max();
    let min = constraints.min();
    let width = if max.width().is_finite() {
        max.width()
    } else {
        min.width()
    };
    let height = if max.height().is_finite() {
        max.height()
    } else {
        min.height()
    };
    constraints.constrain(Size::new(width, height).expect("constraint extents are valid"))
}

fn tight(width: f32, height: f32) -> Constraints {
    Constraints::tight(Size::new(width, height).expect("shell child bounds are valid"))
        .expect("shell constraints are valid")
}
