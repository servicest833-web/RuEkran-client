param(
    [string]$OutputDir,
    [string]$PayloadDir,
    [ValidateRange(0,11)][int]$CompressionLevel = 11
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\environment-x86.ps1"
$workspace = Split-Path $PSScriptRoot
$version = Get-Content "$workspace\release\version.json" -Raw | ConvertFrom-Json
$sourceRoot = "$workspace\source-x86"
$payload = if ($PayloadDir) { $PayloadDir } else { "$workspace\package\RuEkran-$($version.display)-x86-delivery" }
$dist = if ($OutputDir) { $OutputDir } else { "$workspace\dist\Windows" }
if (Test-Path -LiteralPath $payload) { throw 'Payload exists; inspect it before repackaging.' }
if (-not (Test-Path -LiteralPath "$sourceRoot\target\release\rustdesk.exe")) { throw 'Build the x86 client first.' }
New-Item -ItemType Directory -Force -Path $payload,$dist | Out-Null
Copy-Item "$sourceRoot\target\release\rustdesk.exe" "$payload\RuEkran.exe"
Copy-Item "$workspace\downloads\sciter-x86.dll" "$payload\sciter.dll"
New-Item -ItemType Directory -Path "$payload\usbmmidd_v2" | Out-Null
Get-ChildItem "$workspace\downloads\usbmmidd-unpacked\usbmmidd_v2" |
    Where-Object Name -NotIn 'deviceinstaller.exe','usbmmidd.bat' |
    Copy-Item -Destination "$payload\usbmmidd_v2" -Recurse
Copy-Item "$sourceRoot\LICENCE" "$payload\LICENCE.txt"
Copy-Item "$sourceRoot\LICENCE" "$payload\AGPL-3.0.txt"
$notice = Get-Content "$sourceRoot\flutter\assets\RuEkran-NOTICE.txt" -Raw -Encoding utf8
$notice += "`nВерсия RuЭкран: $($version.display). Интерфейс этой сборки: Sciter x86."
[IO.File]::WriteAllText("$payload\RuEkran-NOTICE.txt", $notice, [Text.UTF8Encoding]::new($true))
Copy-Item "$workspace\downloads\Sciter-license.htm" "$payload\Sciter-license.htm"
python "$PSScriptRoot\archive-source.py" "$payload\source\RuEkran-source.zip" $sourceRoot
if ($LASTEXITCODE -ne 0) { throw 'Source archive failed.' }
Set-Location "$sourceRoot\libs\portable"
python generate.py -l $CompressionLevel -f $payload -o . -e "$payload\RuEkran.exe"
if ($LASTEXITCODE -ne 0) { throw 'Portable packaging failed.' }
Copy-Item "$sourceRoot\target\release\rustdesk-portable-packer.exe" "$dist\RuEkran-Windows-x86-x64.exe"
Copy-Item "$payload\source\RuEkran-source.zip","$payload\LICENCE.txt","$payload\RuEkran-NOTICE.txt","$payload\Sciter-license.htm" $dist
Get-ChildItem $dist -File | Where-Object Name -ne 'SHA256SUMS.txt' | Get-FileHash -Algorithm SHA256 |
    ForEach-Object { "$($_.Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($_.Path))" } |
    Set-Content "$dist\SHA256SUMS.txt" -Encoding ascii
