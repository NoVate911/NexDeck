# Руководство разработчика

## Требования к окружению

- **Rust (`stable`):** установите через [rustup.rs](https://rustup.rs/) с компонентами `rustfmt` и `clippy`.
- **Линкер MSVC на Windows (`link.exe`):** для сборки и запуска тестов на Windows необходима установка Visual Studio или Build Tools for Visual Studio с нагрузкой **«Разработка классических приложений на C++»** и Windows 11 SDK.
- **Git и GitHub CLI (`gh`):** для работы с ветками, Issue и PR.
- **С Этапа 2:** Flutter SDK для разработки `apps/mobile`.

---

## Локальная проверка перед PR

```powershell
git clone https://github.com/NoVate911/NexDeck.git
cd NexDeck

# Проверка форматирования, компиляции, clippy, тестов, документации и сборки
./scripts/verify.ps1
```

На Linux / macOS:

```bash
./scripts/verify.sh
```

### Проверка бинарников Stage 0

```powershell
cargo run -p nexdeck-core -- --version
cargo run -p nexdeck-desktop -- --version
```

---

## Порядок создания PR

1. Создайте или выберите GitHub Issue.
2. Отведите ветку от `main` (`feature/*`, `fix/*`, `refactor/*`, `docs/*`, `test/*`, `ci/*`).
3. Пишите коммиты по **Conventional Commits** на английском (`feat(core): ...`).
4. Обновите [`JOURNAL.md`](../../JOURNAL.md).
5. Запустите `./scripts/verify.ps1`.
6. Откройте PR на русском языке с `Closes #<номер>`, дождитесь прохождения `CI Gate` и проверок безопасности и выполните **Squash and Merge**.
