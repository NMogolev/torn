# Torn

Этот документ описывает API, который уже реализован в Torn. На текущем этапе
Torn работает без нативного окна: приложение строит дерево виджетов, выполняет
раскладку, записывает команды отрисовки в `DisplayList`, передаёт поинтеры
и может экспортировать результат рендера в PNG.

Полностью запускаемый исходник находится в
[`crates/torn/examples/headless_tutorial.rs`](../crates/torn/examples/headless_tutorial.rs).

## Быстрый запуск

В корне репозитория выполните:

```text
cargo run -p torn --example headless_tutorial
```

Пример напечатает число обработанных нажатий и создаст
`target/torn-tutorial.png`.

## 1. Импорты через единый фасад

Потребительский код зависит от crate `torn`, а не от `torn-core`, `torn-ui` или
`torn-widgets` по отдельности:

```rust
use torn::{
    Box as TornBox, Button, Color, Constraints, Point, Size, Text, UiRuntime,
    render::{DisplayList, PaintContext, TextLayout},
    software::{PixelBuffer, SoftwareRenderer},
};
```

Основные типы (`Box`, `Button`, `Text`, `Row`, `Column`, `UiRuntime`,
`Constraints`) реэкспортируются из корня. Более низкоуровневые контракты лежат
в следующих модулях:

- `torn::render` — `DisplayList`, `PaintContext`, `TextLayout`, `TextShaper`;
- `torn::software` — `SoftwareRenderer`, `PixelBuffer` и PNG-экспорт.

## 2. Текст

`Text` принимает не строку, а заранее подготовленный `TextLayout`:

```rust
let label = Text::new(TextLayout::new(
    Size::new(120.0, 16.0)?,
    Color::BLACK,
));
```

Это сознательное разделение ответственности: шейпер измеряет и подготавливает
текст, а виджет использует уже известный размер в layout. Конкретного
`TextShaper` пока нет, поэтому в примерах `TextLayout` создаётся вручную.

Важно: рендерер пока **не растеризует** `DrawText`. Текст присутствует
в `DisplayList`, но в экспортированном PNG его ещё не будет. Фоны `Box` и
`Button` будут видны.

## 3. Button и Box

`Button` принимает один дочерний виджет, добавляет отступ 8 px с каждой
стороны и обрабатывает primary pointer events:

```rust
let mut button = Button::new(label);
button.set_backgrounds(
    Color::rgba8(180, 220, 255, 255),
    Color::rgba8(120, 180, 230, 255),
);
button.set_on_click(|| println!("Нажато"));
```

На текущем этапе коллбек вызывается по событию PointerDown основной кнопки. 
PointerUp сбрасывает визуальное состояние pressed. 
Захват указателя и проверка того, что отпускание произошло в пределах кнопки,
будут добавлены позже вместе с полноценной событийной моделью.


`Box` похож на минимальный `div`: может содержать одного потомка и рисовать фон.
Внешний `Box` в примере задаёт белый холст:

```rust
let mut root = TornBox::with_child(button);
root.set_background(Some(Color::WHITE));
```

## 4. Layout и отрисовка

`UiRuntime` владеет корневым виджетом. Перед обработкой событий или отрисовкой
нужно вызвать `layout`:

```rust
let mut runtime = UiRuntime::new(root);
let canvas = Size::new(320.0, 180.0)?;
runtime.layout(Constraints::tight(canvas)?)?;

let mut display_list = DisplayList::new();
runtime.paint(&mut PaintContext::new(&mut display_list))?;
```

`Constraints::tight(canvas)` задаёт корню точный размер холста. У `Box` ребёнок
остаётся в левом верхнем углу, но фон растягивается на весь итоговый размер.

Порядок фрейма сейчас такой:

```text
состояние виджета -> layout -> InputEvent -> paint -> DisplayList -> renderer
```

Если состояние виджета изменилось через `UiRuntime::root_mut()`, предыдущая
раскладка становится недействительной; перед следующим `paint` и
`dispatch_event` снова вызовите `layout`.

## 5. Передача клика

События имеют координаты относительно корня. Рантайм выполняет hit-test и
передаёт виджету локальные координаты:

```rust
use torn::{
    InputEvent, Modifiers, Point, PointerButton, PointerButtons, PointerEvent, PointerId,
};

let event = InputEvent::PointerDown(PointerEvent {
    pointer_id: PointerId(1),
    position: Point::new(12.0, 12.0),
    button: Some(PointerButton::Primary),
    buttons: PointerButtons::PRIMARY,
    modifiers: Modifiers::NONE,
});

let result = runtime.dispatch_event(&event);
assert!(result.is_handled());
```

Сейчас маршрутизация прямая: событие получает самый глубокий виджет под
указателем. Нет bubble/capture, фокуса, keyboard routing и pointer capture.

## 6. Diagnostics вместо неясного падения

`UiRuntime` изолирует panic в пользовательских методах `Widget::layout`,
`Widget::paint` и `Widget::handle_event`. Вместо распространения panic наружу
`layout` и `paint` возвращают `UiRuntimeError::WidgetPanicked`, а runtime
сохраняет структурированный `Diagnostic`:

```rust
match runtime.layout(Constraints::tight(canvas)?) {
    Ok(_) => {}
    Err(error) => {
        for diagnostic in runtime.take_diagnostics() {
            eprintln!("{diagnostic}");
        }
        return Err(error.into());
    }
}
```

По умолчанию diagnostics собираются внутри runtime. Чтобы одновременно
пересылать их в собственный логгер, установите reporter:

```rust
runtime.set_diagnostic_reporter(|diagnostic: &torn::Diagnostic| {
    eprintln!("{diagnostic}");
});
```

В строгих тестах reporter `PanicOnDiagnostic` превращает любую diagnostic в
панику. Это позволяет не пропустить ошибку разработчика в CI:

```rust
runtime.set_diagnostic_reporter(torn::PanicOnDiagnostic);
```

`SoftwareRenderer` сохраняет обычный `Result`, но имеет
`render_with_diagnostics`. Он сообщает об ошибках display list, а также выдаёт
warning, если пропускает `DrawText` из-за пока отсутствующей растеризации
глифов:

```rust
let mut diagnostics = Vec::new();
SoftwareRenderer.render_with_diagnostics(&display_list, &mut pixels, &mut diagnostics)?;
```

## 7. Рендер в PNG

`SoftwareRenderer` выполняет команды `FillRect`, clip-операции и source-over
композицию в `PixelBuffer`. Затем `PixelBuffer::write_png` создаёт
детерминированный RGBA PNG:

```rust
let mut pixels = PixelBuffer::new(320, 180)?;
SoftwareRenderer.render(&display_list, &mut pixels)?;
pixels.write_png("target/torn-tutorial.png")?;
```
