$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

python .\scripts\validate_repository.py
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked

Write-Host 'lasso-zen-bre tests passed (Windows)'
