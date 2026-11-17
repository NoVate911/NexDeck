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

if (Get-Command flutter -ErrorAction SilentlyContinue) {
    Write-Host "==> Verifying Flutter mobile client..."
    Push-Location apps/mobile
    try {
        dart format --output=none --set-exit-if-changed lib test
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        flutter analyze --fatal-infos
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        flutter test
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    finally {
        Pop-Location
    }
}
else {
    Write-Host "==> Flutter SDK not found in PATH; skipping mobile checks."
}

Write-Host "All local verification checks passed."
