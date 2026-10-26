# Install the latest atlas for Windows into $env:ATLAS_INSTALL_DIR (default %LOCALAPPDATA%\Programs\atlas)
# and put that folder on the user's PATH. $env:ATLAS_CHANNEL = 'nightly' installs the nightly build of `next`,
# as `atlas-nightly.exe` beside the release's `atlas.exe`.
$ErrorActionPreference = 'Stop'
$target = 'x86_64-pc-windows-msvc'
$dir = if ($env:ATLAS_INSTALL_DIR) { $env:ATLAS_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\atlas' }
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$zip = Join-Path ([IO.Path]::GetTempPath()) "atlas-$target.zip"
$nightly = $env:ATLAS_CHANNEL -eq 'nightly'
$from = if ($nightly) { 'download/nightly' } else { 'latest/download' }
$name = if ($nightly) { 'atlas-nightly' } else { 'atlas' }
Invoke-WebRequest -UseBasicParsing "https://github.com/raoulg/atlas/releases/$from/atlas-$target.zip" -OutFile $zip
$work = Join-Path ([IO.Path]::GetTempPath()) "atlas-$target"
Expand-Archive -Force $zip $work
Move-Item -Force (Join-Path $work 'atlas.exe') (Join-Path $dir "$name.exe")
Remove-Item -Recurse $zip, $work
Write-Host "atlas installed in $dir\$name.exe"
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($path -split ';') -notcontains $dir) {
  [Environment]::SetEnvironmentVariable('Path', $(if ($path) { "$path;$dir" } else { $dir }), 'User')
  Write-Host "added $dir to your PATH; open a new terminal to run '$name'"
}
