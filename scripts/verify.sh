#!/usr/bin/env bash
set -euo pipefail

echo "==> Checking formatting..."
cargo fmt --all -- --check

echo "==> Checking compilation..."
cargo check --workspace --all-targets --locked

echo "==> Running clippy..."
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "==> Running tests..."
cargo test --workspace --locked

echo "==> Checking documentation..."
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

echo "==> Building release binaries..."
cargo build --workspace --release --locked

if command -v flutter >/dev/null 2>&1; then
    echo "==> Verifying Flutter mobile client..."
    (
        cd apps/mobile
        dart format --output=none --set-exit-if-changed lib test
        flutter analyze --fatal-infos
        flutter test
    )
else
    echo "==> Flutter SDK not found in PATH; skipping mobile checks."
fi

echo "All local verification checks passed."
