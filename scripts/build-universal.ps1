param([switch]$Rebuild)
$ErrorActionPreference = 'Stop'
if ($Rebuild) { python "$PSScriptRoot\prepare-release.py" --reuse }
else { python "$PSScriptRoot\prepare-release.py" }
if ($LASTEXITCODE -ne 0) { throw 'Release version preparation failed.' }
. "$PSScriptRoot\environment-x86.ps1"
$workspace = Split-Path $PSScriptRoot
Set-Location "$workspace\source-x86"
python res/inline-sciter.py
if ($LASTEXITCODE -ne 0) { throw 'Sciter UI generation failed.' }
cargo build --locked --offline --features inline,vram,hwcodec --release --bins
if ($LASTEXITCODE -ne 0) { throw 'x86 client compilation failed.' }
