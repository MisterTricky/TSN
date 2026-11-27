$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $repoRoot "Cargo.toml"

cargo clean --manifest-path $manifest

Write-Host "Removed this workspace's Cargo target artifacts. Sources and Cargo.lock were left intact."
