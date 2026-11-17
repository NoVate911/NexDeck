# Мобильный клиент NexDeck (Flutter)

Кроссплатформенный клиент для **Android**, **iOS** и отладочного запуска на **Windows** ([ADR-0003](../../docs/architecture/adr/0003-mobile-client-flutter.md)).

## Реализовано (Этап 2 завершён)

- **Модели протокола и мониторинга (`lib/src/models/`):** точная типизированная десериализация JSON-контрактов `nexdeck-transport` (`Envelope`, `MessageKind`, `ControlAction`, `ControlCommandRequest`, `ControlCommandResponse`, `HealthResponse`, `PairingQrPayload`, `PairRequest`, `PairedCredentials`) и `nexdeck-monitoring` (`SystemSnapshot`, CPU, RAM, GPU, диски, сеть, батарея).
- **Транспорт с пиннингом TLS (`lib/src/transport/`):**
  - Проверка SHA-256 отпечатка DER-сертификата хоста (`X509Certificate.der`) через пустой `SecurityContext(withTrustedRoots: false)` и constant-time сравнение;
  - Запросы `GET /health` и `POST /pair`;
  - Абстрактный интерфейс `NexDeckTransport` и реализация `WssNexDeckTransport` (`wss://<host>:<port>/ws` с `Authorization: Bearer <device_token>`).
- **Хранение сессии, история и автопереподключение (`lib/src/storage/`, `lib/src/state/`):** сохранение учётных данных, буферы истории CPU, RAM и сети TX/RX, отправка команд `ControlAction` и экспоненциальное переподключение при обрыве WSS.
- **UI и графики (`lib/src/ui/`):**
  - Экран сопряжения (быстрый импорт QR JSON из `nexdeck-agent --daemon --pair` или ручной ввод).
  - Живые графики в реальном времени (`TelemetryLineChart` на `CustomPainter`), детализация по логическим ядрам CPU, GPU VRAM и трафику сети, панель удалённого управления мультимедиа и громкостью (`VolumeUp`, `VolumeDown`, `VolumeMuteToggle`, `MediaPlayPause`, `MediaNextTrack`, `MediaPreviousTrack`).

## Локальные проверки

```powershell
cd apps/mobile
flutter pub get
dart format --output=none --set-exit-if-changed lib test
flutter analyze --fatal-infos
flutter test
```

При установленной Visual Studio на Windows клиент также можно запустить для локальной проверки против `nexdeck-agent.exe --daemon --pair`:

```powershell
cd apps/mobile
flutter run -d windows
```
