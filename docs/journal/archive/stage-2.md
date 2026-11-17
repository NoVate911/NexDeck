# Этап 2 — Mobile Client (архив)

Период: инициализация кроссплатформенного Flutter-клиента `apps/mobile`, защищённое сопряжение и потоковая визуализация телеметрии + управление мультимедиа (`PR #19`, `Issue #20`).

## Выполненные задачи

1. **Каркас, TLS-пиннинг и сопряжение (`apps/mobile`, [PR #19](https://github.com/NoVate911/NexDeck/pull/19)):**
   - Создан Flutter-проект `nexdeck_mobile` (`org.example.nexdeck_mobile`, платформы `android`, `ios`, `windows`) на Material 3 (`package:material_ui/material_ui.dart` для Flutter 3.50.0 / Dart 3.14.0).
   - Реализованы типизированные Dart-модели `Envelope`, `MessageKind`, `HealthResponse`, `PairingQrPayload`, `PairRequest`, `PairedCredentials`, `SystemSnapshot` и метрики без `dynamic`.
   - Реализован HTTP/WSS-клиент поверх `dart:io` с пустым `SecurityContext(withTrustedRoots: false)`, проверкой SHA-256 отпечатка DER-сертификата хоста (constant-time), `SessionStore` и экспоненциальным автопереподключением в `AgentController`.
   - В GitHub Actions добавлена обязательная проверка `Mobile Client (Flutter)` (форматирование, `flutter analyze --fatal-infos`, `flutter test`), а в `dependency-review.yml` настроено исключение `allow-dependencies-licenses` для проверенных `pub`-пакетов со строчными SPDX-строками.
2. **Графики в реальном времени, детализация и управление мультимедиа (`Issue #20`):**
   - В `crates/transport` (`control.rs`) и `crates/core` (`control.rs`, `runtime.rs`) добавлен строгий белый список команд `ControlAction` (`VolumeUp`, `VolumeDown`, `VolumeMuteToggle`, `MediaPlayPause`, `MediaNextTrack`, `MediaPreviousTrack`), нативный Windows-исполнитель `WindowsMediaExecutor` на базе `SendInput` (`VK_VOLUME_*` / `VK_MEDIA_*` без обхода UAC) и маршрутизация входящих `MessageKind::ControlCommand` с отправкой `ControlCommandResponse` обратно клиентам.
   - В `apps/mobile` реализован собственный лёгкий `TelemetryLineChart` на базе `CustomPainter` (CPU %, RAM %, двухсерийный сетевой график RX/TX KiB/с), раскрываемая детализация по логическим ядрам CPU, блок GPU VRAM/частоты/температуры, суммарный сетевой трафик и панель 6 кнопок управления мультимедиа и громкостью.

## Проверки этапа

- Rust: `cargo fmt`, `cargo clippy -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `nexdeck-core` (14/14), `nexdeck-transport` (10/10), `nexdeck-integration-tests` (stage 0/1/2), `nexdeck-agent.exe` (`1 743 872` байт, `--measure-ram`: `private=1.23 MiB`).
- Flutter: `dart format`, `flutter analyze --fatal-infos` (0 замечаний), `flutter test` (13/13 пройдены), release-сборка Windows `nexdeck_mobile.exe`.
