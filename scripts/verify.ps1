$ErrorActionPreference = 'Stop'

Write-Host "==> Checking formatting..."
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Checking compilation..."
cargo check --workspace --all-targets --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Running clippy..."
cargo clippy --workspace --all-targets --locked -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Running tests..."
cargo test --workspace --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Checking documentation..."
$prevRustdocFlags = $env:RUSTDOCFLAGS
$env:RUSTDOCFLAGS = "-D warnings"
try {
    cargo doc --workspace --no-deps --locked
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    if ($null -eq $prevRustdocFlags) {
        Remove-Item Env:RUSTDOCFLAGS -ErrorAction SilentlyContinue
    }
    else {
        $env:RUSTDOCFLAGS = $prevRustdocFlags
    }
}

Write-Host "==> Building release binaries..."
cargo build --workspace --release --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "All local verification checks passed."
