# Install the latest atlas for Windows into $env:ATLAS_INSTALL_DIR (default %LOCALAPPDATA%\Programs\atlas)
# and put that folder on the user's PATH. $env:ATLAS_CHANNEL = 'nightly' installs the nightly build of `next`.
$ErrorActionPreference = 'Stop'
$target = 'x86_64-pc-windows-msvc'
$dir = if ($env:ATLAS_INSTALL_DIR) { $env:ATLAS_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\atlas' }
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$zip = Join-Path ([IO.Path]::GetTempPath()) "atlas-$target.zip"
$from = if ($env:ATLAS_CHANNEL -eq 'nightly') { 'download/nightly' } else { 'latest/download' }
Invoke-WebRequest -UseBasicParsing "https://github.com/raoulg/atlas/releases/$from/atlas-$target.zip" -OutFile $zip
Expand-Archive -Force $zip $dir
Remove-Item $zip
Write-Host "atlas installed in $dir\atlas.exe"
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($path -split ';') -notcontains $dir) {
  [Environment]::SetEnvironmentVariable('Path', $(if ($path) { "$path;$dir" } else { $dir }), 'User')
  Write-Host "added $dir to your PATH; open a new terminal to run 'atlas'"
}
