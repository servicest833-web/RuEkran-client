$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$source = Join-Path $root 'source-x86'
$out = Join-Path $root 'dist\ci-windows'
if (Test-Path -LiteralPath $out) { throw 'Artifact output already exists; use a fresh runner' }
$version = Get-Content -LiteralPath (Join-Path $root 'release\version.json') -Raw | ConvertFrom-Json
if ($version.display -ne '0.1.0-beta.2' -or $version.build -ne 24) { throw 'Expected Beta 2 source' }

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Visual Studio C++ tools unavailable' }
Import-Module (Join-Path $vs 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments '-arch=x86 -host_arch=x64'
Push-Location $source
try {
    & python res/inline-sciter.py
    if ($LASTEXITCODE -ne 0) { throw 'Interface generation failed' }
    & cargo fetch --locked
    if ($LASTEXITCODE -ne 0) { throw 'Locked Rust dependency download failed' }
    & cargo build --locked --offline --release --bins --features inline,vram,hwcodec
    if ($LASTEXITCODE -ne 0) { throw 'Windows compilation failed' }
} finally { Pop-Location }

New-Item -ItemType Directory -Path $out -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $source 'target\release\rustdesk.exe') -Destination (Join-Path $out 'RuEkran.exe')
Copy-Item -LiteralPath (Join-Path $source 'LICENCE') -Destination (Join-Path $out 'AGPL-3.0.txt')
Copy-Item -LiteralPath (Join-Path $source 'flutter\assets\RuEkran-NOTICE.txt') -Destination (Join-Path $out 'RuEkran-NOTICE.txt')
Copy-Item -LiteralPath (Join-Path $root '.ci\downloads\sciter.dll'),(Join-Path $root '.ci\downloads\Sciter-license.htm') -Destination $out
Expand-Archive -LiteralPath (Join-Path $root '.ci\downloads\usbmmidd_v2.zip') -DestinationPath (Join-Path $root '.ci\driver')
Copy-Item -LiteralPath (Join-Path $root '.ci\driver\usbmmidd_v2') -Destination $out -Recurse

$commit = (& git -C $root rev-parse HEAD).Trim()
$provenance = [ordered]@{
    version=$version.display; build=$version.build; commit=$commit
    repository=$env:GITHUB_REPOSITORY; run_id=$env:GITHUB_RUN_ID; run_attempt=$env:GITHUB_RUN_ATTEMPT
    source_archive_sha256=(Get-Content (Join-Path $root 'SOURCE-MANIFEST.json') -Raw | ConvertFrom-Json).published_source_archive_sha256
    rust=(& rustc --version); target='i686-pc-windows-msvc'
    features='inline,vram,hwcodec'; vcpkg_commit=(& git -C $env:VCPKG_ROOT rev-parse HEAD)
    unsigned=$true; public_release=$false; signpath_eligibility_confirmed=$false
}
$provenance | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $out 'BUILD-PROVENANCE.json') -Encoding utf8
@"
RuEkran $($version.display) - unsigned CI build
Source commit: $commit
https://github.com/servicest833-web/RuEkran-client/tree/$commit

Start RuEkran.exe with its companion files in the same directory.
This is a fresh build from source, not the byte-identical public installer.
No website deployment, updater publication or GitHub Release was performed.

This Application uses Sciter Engine (https://sciter.com/), copyright Terra Informatica Software, Inc.
Sciter remains a proprietary third-party dependency. See Sciter-license.htm.
This CI build does not claim acceptance by or a signature from SignPath Foundation.
"@ | Set-Content -LiteralPath (Join-Path $out 'README.txt') -Encoding utf8

Get-ChildItem -LiteralPath $out -File -Recurse | Sort-Object FullName | ForEach-Object {
    "$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($_.FullName.Substring($out.Length + 1).Replace('\','/'))"
} | Set-Content -LiteralPath (Join-Path $out 'SHA256SUMS.txt') -Encoding ascii
