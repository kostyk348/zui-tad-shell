# legacy/ — прежний стек (не собирается, оставлен для истории)

| Крейт | Что это | Почему здесь |
|---|---|---|
| `shell-app` | ранняя оболочка на winit + softbuffer | заменена `crates/phosphor` + bin `zui-preview`; файл не компилировался (лишняя скобка) |
| `editor-core` | TAD-редакторы (текст/таблица/вектор), undo/redo, порталы | относились к shell-app |
| `native-apps` | встроенные приложения (Files, Calculator, Settings, Pins) | относились к shell-app |
| `skia-renderer` | CPU-рендер холста (tiny-skia + ab_glyph) | вытеснен `crates/phosphor` |

## Известная потеря

`editor-core/src/editors.rs` (≈537 строк: `EditorRegistry`, `TextEditor`,
`TableEditor`, `VectorEditor`, `ToolCommand`, undo/redo) **утрачен**: файл был
обнулён неудачной однострочной правкой до первого коммита. Остальные файлы
крейта целы. Восстановление: переписать редакторы заново (структура ясна из
`editor-core/src/lib.rs` и `focus.rs`).

Продуктовая часть после разделения: `crates/{tad-core,canvas-engine,de-common,compositor,phosphor}`.
