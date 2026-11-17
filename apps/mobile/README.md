# Мобильный клиент NexDeck (Flutter)

Кроссплатформенный клиент для **Android**, **iOS** и отладочного запуска на **Windows** ([ADR-0003](../../docs/architecture/adr/0003-mobile-client-flutter.md)).

## Реализовано (Этап 2.1)

- **Модели протокола и мониторинга (`lib/src/models/`):** точная типизированная десериализация JSON-контрактов `nexdeck-transport` (`Envelope`, `MessageKind`, `HealthResponse`, `PairingQrPayload`, `PairRequest`, `PairedCredentials`) и `nexdeck-monitoring` (`SystemSnapshot`, CPU, RAM, GPU, диски, сеть, батарея).
- **Транспорт с пиннингом TLS (`lib/src/transport/`):**
  - Проверка SHA-256 отпечатка DER-сертификата хоста (`X509Certificate.der`) через пустой `SecurityContext(withTrustedRoots: false)` и constant-time сравнение;
  - Запросы `GET /health` и `POST /pair`;
  - Абстрактный интерфейс `NexDeckTransport` и реализация `WssNexDeckTransport` (`wss://<host>:<port>/ws` с `Authorization: Bearer <device_token>`).
- **Хранение сессии и автопереподключение (`lib/src/storage/`, `lib/src/state/`):** сохранение учётных данных в JSON-файле, буфер истории CPU и автоматическое переподключение при обрыве WSS.
- **UI (`lib/src/ui/`):** экран сопряжения (быстрый импорт QR JSON из `nexdeck-agent --daemon --pair` или ручной ввод) и базовый экран мониторинга подключённого хоста.

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
