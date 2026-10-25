$ErrorActionPreference = 'Stop'
$repo = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $repo
try {
    cargo build -p atlas
    $triple = (rustc --print host-tuple).Trim()
    $binaries = Join-Path $PSScriptRoot 'src-tauri/binaries'
    New-Item -ItemType Directory -Path $binaries -Force | Out-Null
    Copy-Item 'target/debug/atlas.exe' (Join-Path $binaries "atlas-$triple.exe") -Force
} finally {
    Pop-Location
}
