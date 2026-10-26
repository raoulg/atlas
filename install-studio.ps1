# Install the latest Atlas Studio for Windows, for this user alone (no administrator rights), with the
# atlas of the same release inside it. $env:ATLAS_CHANNEL = 'nightly' installs the nightly build of `next`:
# an app of its own, Atlas Studio Nightly, beside the release.
#
# Studio is not signed with a paid identity. A setup program a browser downloads is marked as from the
# internet, and SmartScreen then asks first ("More info", "Run anyway"); what this script downloads
# carries no such mark, so it runs as it is.
$ErrorActionPreference = 'Stop'
$target = 'x86_64-pc-windows-msvc'
$nightly = $env:ATLAS_CHANNEL -eq 'nightly'
$from = if ($nightly) { 'download/nightly' } else { 'latest/download' }
$app = if ($nightly) { 'Atlas Studio Nightly' } else { 'Atlas Studio' }
$setup = Join-Path ([IO.Path]::GetTempPath()) "atlas-studio-$target-setup.exe"
Invoke-WebRequest -UseBasicParsing "https://github.com/raoulg/atlas/releases/$from/atlas-studio-$target-setup.exe" -OutFile $setup
Start-Process -Wait -FilePath $setup -ArgumentList '/S'
Remove-Item $setup
Write-Host "$app installed in $(Join-Path $env:LOCALAPPDATA $app); find it in the Start menu"
