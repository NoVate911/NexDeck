# NexDeck Desktop Manager

Отдельная графическая оболочка настройки под Windows 11 (`nexdeck-desktop`) на **чистом Rust UI** (`egui` / `slint`, см. [ADR-0002](../../docs/architecture/adr/0002-desktop-ui-pure-rust.md)).

## Архитектурная граница

- Запускается как отдельный процесс по требованию пользователя и не удерживает GUI-память внутри фонового `nexdeck-agent`.
- Взаимодействует с `Core Agent` через `nexdeck-api` по локальному IPC (`localhost` / Named Pipes).
- Отвечает за настройку подключённых устройств, политик безопасности, панелей и расширений.
