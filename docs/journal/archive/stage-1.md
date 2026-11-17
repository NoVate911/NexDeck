# Этап 1 — Core Agent (архив)

Период: реализация фонового Windows-агента, нативного мониторинга и защищённого сетевого слоя (`PR #13`, `PR #15`, `Issue #16`).

## Выполненные задачи

1. **Нативный мониторинг (`crates/monitoring`, [PR #13](https://github.com/NoVate911/NexDeck/pull/13)):**
   - Реализован `WindowsCollector` поверх `windows-sys 0.61.2` (`Win32/PDH`): общая и по-ядерная загрузка CPU, базовая частота (`CallNtPowerInformation`), RAM (`GlobalMemoryStatusEx`), логические диски и PDH R/W, сетевые интерфейсы (`GetIfTable2` через `NonNull`-обёртку `MibTableGuard`), питание от батареи (`GetSystemPowerStatus`) и `uptime` (`GetTickCount64`).
   - Добавлен переносимый `StaticCollector` для тестов и Linux CI.
2. **Транспорт и сопряжение (`crates/transport`, [PR #15](https://github.com/NoVate911/NexDeck/pull/15)):**
   - Согласован сетевой стек `tokio` (`current_thread`) + `axum` + `axum-server` + `rustls` (`ring`) + `rcgen` ([ADR-0005](../../architecture/adr/0005-network-stack-tokio-axum-wss.md)).
   - Реализованы `HostTlsIdentity` (генерация ECDSA самоподписанного сертификата и SHA-256 отпечатка), `DeviceRegistry` (CSPRNG 6-значный PIN без modulo bias, лимит попыток, TTL, постоянная проверка времени и хранение только SHA-256 хэшей токенов) и HTTPS/WSS-сервер (`/health`, `/pair`, `/ws`).
   - Поднят `rust-version = "1.88"` для совместимости с `time >= 0.3.47` (`RUSTSEC-2026-0009`).
3. **Runtime Core Agent и CLI (`crates/core`, `Issue #16`):**
   - Добавлена сериализация `SystemSnapshot` через `serde` (`crates/monitoring`).
   - Реализованы постоянное локальное хранилище `StoragePaths` (`config.json`, `devices.json`, PEM сертификат/ключ и SHA-256 отпечаток), управление автозапуском Windows (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` без обхода UAC), асинхронный цикл `spawn_agent_runtime` (`tokio` `current_thread`, рассылка `MessageKind::Telemetry`, приём `Heartbeat`, сохранение новых устройств), сборщик памяти процесса `process_metrics` и расширенный CLI `nexdeck-agent`.

## Замеры release-сборки Windows 11 (`0.1.0`)

- Размер бинарника `nexdeck-agent.exe` (`opt-level = "z"`, LTO, strip): **1 738 752 байт (~1.66 MiB)**.
- `--measure-ram`: `working_set=5.44 MiB`, `peak=5.49 MiB`, `private=1.16 MiB`.
- `--daemon --bind 127.0.0.1:0 --pair`: `working_set=6.53 MiB`, `private=1.18 MiB` (приватная память укладывается в бюджет 2–5 МБ; остальная часть Working Set приходится на разделяемые системные DLL Windows и PDH).
