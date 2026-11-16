# NexDeck (Русская версия)

> Открытая модульная платформа для лёгкого мониторинга Windows 11, настраиваемых панелей управления в стиле Stream Deck, централизованных уведомлений, удалённого управления и подтверждения действий с мобильного устройства.

[English README](../README.md) · [Техническое задание (TZ.md)](../TZ.md) · [Архитектура](architecture/overview.md) · [Дорожная карта](architecture/roadmap.md)

---

## Текущий статус

Проект **NexDeck** проходит **Этап 0 (Подготовка репозитория и архитектуры)** согласно [TZ.md v0.3.0](../TZ.md).  
На этом этапе подготовлена модульная структура Cargo Workspace (`0.1.0`), зафиксированы архитектурные решения (ADR), контракты подсистем, политика лицензирования, документация и полный контур автоматизации CI/CD и безопасности GitHub.

## Ключевые принципы

- **Минимальное потребление ресурсов:** фоновый агент на **Rust** с ориентиром **2–5 МБ RAM в простое**, минимальной нагрузкой на CPU и без тяжёлого GUI в фоновом процессе.
- **Модульная архитектура:** чёткое разделение на фоновый `Core Agent`, отдельный `Desktop Manager` на чистом Rust UI, мобильный клиент на **Flutter** (Android/iOS), абстрактный транспортный слой и изолированный `Plugin Engine` на **WebAssembly**.
- **Независимость от транспорта:** Wi-Fi / WebSocket в MVP с единой абстракцией для последующей поддержки USB и Bluetooth.
- **Безопасность:** авторизация устройств, токены с ограничением прав, одноразовые идентификаторы и таймауты в `Remote Approval`, изоляция плагинов с квотами ресурсов, отсутствие подмены штатного Windows UAC.

## Выбранный технологический стек

| Компонент | Технология | Документ |
| --- | --- | --- |
| **Core Agent и подсистемы** | Rust (Edition 2024) | [ADR-0001](architecture/adr/0001-core-rust-and-workspace.md) |
| **Desktop Manager (Windows 11)** | Чистый Rust UI (`egui` / `slint`) | [ADR-0002](architecture/adr/0002-desktop-ui-pure-rust.md) |
| **Mobile Client (Android / iOS)** | Flutter | [ADR-0003](architecture/adr/0003-mobile-client-flutter.md) |
| **Plugin Engine** | WebAssembly (`wasmtime` в изолированном контуре) | [ADR-0004](architecture/adr/0004-plugin-engine-wasm.md) |
| **Лицензия** | Apache License 2.0 | [LICENSE](../LICENSE) |

## Быстрая проверка локально

Требуется установленный [Rust toolchain](https://rustup.rs/) (`stable`):

```powershell
git clone https://github.com/NoVate911/NexDeck.git
cd NexDeck
./scripts/verify.ps1
```

## Полезные ссылки

- [Полное техническое задание (TZ.md)](../TZ.md)
- [Список требований и ограничений](architecture/requirements.md)
- [Архитектура и зависимости компонентов](architecture/overview.md)
- [План разработки и открытые вопросы](architecture/roadmap.md)
- [План тестирования](architecture/testing-plan.md)
- [Модель угроз и безопасность](security/threat-model.md)
- [Руководство разработчика](development/getting-started.md)
- [Правила участия в разработке](../CONTRIBUTING.md)
