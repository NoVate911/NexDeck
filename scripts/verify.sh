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

echo "All local verification checks passed."
