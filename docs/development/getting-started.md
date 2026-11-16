# Getting Started (Developer Guide)

## Prerequisites

- **Rust toolchain (`stable`):** Install via [rustup.rs](https://rustup.rs/) with `rustfmt` and `clippy` components.
- **Windows MSVC Linker (`link.exe`):** Install Visual Studio or Build Tools for Visual Studio with the **Desktop development with C++** workload and Windows 11 SDK.
- **Git & GitHub CLI (`gh`):** Recommended for branch, issue, and PR workflows.
- **Target OS:** Windows 11 (primary runtime), with Linux/macOS supported for portable crate development and CI.
- **Optional (Stage 2+):** Flutter SDK for `apps/mobile`.

---

## Local Setup & Verification

```powershell
git clone https://github.com/NoVate911/NexDeck.git
cd NexDeck

# Run formatting, compilation, clippy, unit/integration/doc tests, and release build
./scripts/verify.ps1
```

On Linux / macOS:

```bash
./scripts/verify.sh
```

### Individual Commands

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
$env:RUSTDOCFLAGS="-D warnings"; cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
```

### Smoke Running Stage 0 Binaries

```powershell
cargo run -p nexdeck-core -- --version
cargo run -p nexdeck-desktop -- --version
```

---

## Branching & Pull Request Workflow

1. Create or reference a GitHub Issue.
2. Branch off `main` using a standard prefix (`feature/*`, `fix/*`, `refactor/*`, `docs/*`, `test/*`, `ci/*`).
3. Commit with [Conventional Commits](https://www.conventionalcommits.org/).
4. Run `./scripts/verify.ps1` locally.
5. Open a PR against `main`, wait for `CI Gate` and all security checks to pass, and use **Squash and Merge**.
