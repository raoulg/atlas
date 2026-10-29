# Install the latest atlas-code for Windows into $env:ATLAS_INSTALL_DIR (default %LOCALAPPDATA%\Programs\atlas-code)
# and put that folder on the user's PATH. $env:ATLAS_CHANNEL = 'nightly' installs the nightly build of `next`,
# as `atlas-code-nightly.exe` beside the release's `atlas-code.exe`. The `atlas.exe` (or `atlas-nightly.exe`)
# an earlier version installed is removed, with its folder when that is left empty: the command is `atlas-code` now.
$ErrorActionPreference = 'Stop'
$target = 'x86_64-pc-windows-msvc'
$programs = Join-Path $env:LOCALAPPDATA 'Programs'
$dir = if ($env:ATLAS_INSTALL_DIR) { $env:ATLAS_INSTALL_DIR } else { Join-Path $programs 'atlas-code' }
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$zip = Join-Path ([IO.Path]::GetTempPath()) "atlas-code-$target.zip"
$nightly = $env:ATLAS_CHANNEL -eq 'nightly'
$from = if ($nightly) { 'download/nightly' } else { 'latest/download' }
$name = if ($nightly) { 'atlas-code-nightly' } else { 'atlas-code' }
$old = if ($nightly) { 'atlas-nightly' } else { 'atlas' }
Invoke-WebRequest -UseBasicParsing "https://github.com/raoulg/atlas/releases/$from/atlas-code-$target.zip" -OutFile $zip
$work = Join-Path ([IO.Path]::GetTempPath()) "atlas-code-$target"
Expand-Archive -Force $zip $work
Move-Item -Force (Join-Path $work 'atlas-code.exe') (Join-Path $dir "$name.exe")
Remove-Item -Recurse $zip, $work
Write-Host "atlas-code installed in $dir\$name.exe"
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
# where an earlier version installed `atlas`: the folder given, or its own default one
$earlier = if ($env:ATLAS_INSTALL_DIR) { $dir } else { Join-Path $programs 'atlas' }
$was = Join-Path $earlier "$old.exe"
if ((Test-Path $was) -and ((& $was --version 2>$null) -match '^atlas 0\.')) {
  Remove-Item -Force $was
  Write-Host "removed the earlier $was; to keep the short name: Set-Alias $old $name"
  if (($earlier -ne $dir) -and -not (Get-ChildItem $earlier)) {
    Remove-Item $earlier
    $path = (($path -split ';') | Where-Object { $_ -and $_ -ne $earlier }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $path, 'User')
  }
}
if (($path -split ';') -notcontains $dir) {
  [Environment]::SetEnvironmentVariable('Path', $(if ($path) { "$path;$dir" } else { $dir }), 'User')
  Write-Host "added $dir to your PATH; open a new terminal to run '$name'"
}
