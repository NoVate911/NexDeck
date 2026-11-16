## Описание изменений

<!-- Кратко опишите, что сделано в этом Pull Request -->

## Причина изменений

<!-- Зачем нужны изменения, какую задачу или проблему они решают -->

## Затронутые компоненты

- [ ] `crates/core`
- [ ] `crates/transport`
- [ ] `crates/monitoring`
- [ ] `crates/plugin_engine`
- [ ] `crates/api`
- [ ] `crates/notifications`
- [ ] `crates/approval`
- [ ] `apps/desktop`
- [ ] `apps/mobile`
- [ ] `sdk` / `examples`
- [ ] `docs` / `CI` / конфигурация репозитория

## Результаты тестирования

<!-- Укажите выполненные локально и в CI проверки (например, ./scripts/verify.ps1) -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check --workspace --all-targets --locked`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`
- [ ] Журнал проекта [`JOURNAL.md`](../JOURNAL.md) обновлён

## Возможные риски

<!-- Несовместимые изменения, риски производительности/безопасности или пометки по проверке -->

## Связанный Issue

Closes #
