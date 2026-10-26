# Build the atlas CLI and place it where Tauri finds its sidecar. `-Release` builds the optimised
# CLI an installed app carries; without it, the debug CLI for `cargo run`.
param([switch]$Release)
$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $repo
try {
    $profile = if ($Release) { 'release' } else { 'debug' }
    if ($Release) { cargo build -p atlas --release } else { cargo build -p atlas }
    $triple = (rustc --print host-tuple).Trim()
    $binaries = Join-Path $PSScriptRoot 'src-tauri/binaries'
    New-Item -ItemType Directory -Path $binaries -Force | Out-Null
    Copy-Item "target/$profile/atlas.exe" (Join-Path $binaries "atlas-$triple.exe") -Force
} finally {
    Pop-Location
}
