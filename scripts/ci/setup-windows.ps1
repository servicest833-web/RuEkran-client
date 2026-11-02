$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$ci = Join-Path $root '.ci'
$downloads = Join-Path $ci 'downloads'
$cache = Join-Path $ci 'vcpkg-binary-cache'
New-Item -ItemType Directory -Force -Path $downloads,$cache | Out-Null

function Get-VerifiedDownload([string]$Url, [string]$Path, [string]$Sha256) {
    if (-not (Test-Path -LiteralPath $Path)) {
        Invoke-WebRequest -Uri $Url -OutFile $Path
    }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Sha256) {
        throw "Download checksum mismatch: $([IO.Path]::GetFileName($Path))"
    }
}

$llvmInstaller = Join-Path $downloads 'LLVM-15.0.6-win32.exe'
Get-VerifiedDownload 'https://github.com/llvm/llvm-project/releases/download/llvmorg-15.0.6/LLVM-15.0.6-win32.exe' $llvmInstaller 'c65efa5d79256162bca1d8f444b8d0816ef57c549e0d5fa5f4a6778b234a19de'
$llvm = Join-Path $ci 'llvm-15-x86'
if (-not (Test-Path -LiteralPath (Join-Path $llvm 'bin\libclang.dll'))) {
    $install = Start-Process -FilePath $llvmInstaller -ArgumentList @('/S', "/D=$llvm") -Wait -PassThru -WindowStyle Hidden
    if ($install.ExitCode -ne 0) { throw "LLVM installation failed: $($install.ExitCode)" }
}

$toolchain = 'nightly-2023-10-13-i686-pc-windows-msvc'
& rustup toolchain install $toolchain --profile minimal --no-self-update --force-non-host
if ($LASTEXITCODE -ne 0) { throw 'Rust toolchain installation failed' }

$vcpkg = Join-Path $ci 'vcpkg'
$vcpkgRevision = '9e593bb18ea69cc5095e012465dcd675a822ed0d'
if (-not (Test-Path -LiteralPath (Join-Path $vcpkg '.git'))) {
    & git clone --filter=blob:none https://github.com/microsoft/vcpkg.git $vcpkg
    if ($LASTEXITCODE -ne 0) { throw 'vcpkg checkout failed' }
}
& git -C $vcpkg checkout --detach $vcpkgRevision
if ($LASTEXITCODE -ne 0) { throw 'Pinned vcpkg revision unavailable' }
& (Join-Path $vcpkg 'bootstrap-vcpkg.bat') -disableMetrics
if ($LASTEXITCODE -ne 0) { throw 'vcpkg bootstrap failed' }

$settings = @{
    RUSTUP_TOOLCHAIN=$toolchain
    LIBCLANG_PATH=(Join-Path $llvm 'bin')
    LLVM_HOME=$llvm
    VCPKG_ROOT=$vcpkg
    VCPKG_INSTALLED_ROOT=(Join-Path $vcpkg 'installed')
    VCPKG_DEFAULT_TRIPLET='x86-windows-static'
    VCPKG_DEFAULT_HOST_TRIPLET='x86-windows-static'
    VCPKG_BINARY_SOURCES="clear;files,$cache,readwrite"
}
foreach ($entry in $settings.GetEnumerator()) {
    [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, 'Process')
    if ($env:GITHUB_ENV) { "$($entry.Key)=$($entry.Value)" | Add-Content -LiteralPath $env:GITHUB_ENV }
}
if ($env:GITHUB_PATH) { (Join-Path $llvm 'bin') | Add-Content -LiteralPath $env:GITHUB_PATH }
$env:Path = (Join-Path $llvm 'bin') + ';' + $env:Path

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Visual Studio C++ tools are required' }
Import-Module (Join-Path $vs 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments '-arch=x86 -host_arch=x64'

Push-Location (Join-Path $root 'source-x86')
try {
    & (Join-Path $vcpkg 'vcpkg.exe') install --triplet x86-windows-static --host-triplet x86-windows-static "--x-install-root=$env:VCPKG_INSTALLED_ROOT" --disable-metrics
    if ($LASTEXITCODE -ne 0) { throw 'Native dependency build failed; inspect vcpkg output' }
} finally { Pop-Location }

& python -m pip install --disable-pip-version-check brotli==1.1.0 pefile==2024.8.26
if ($LASTEXITCODE -ne 0) { throw 'Python build dependencies failed' }

$runtime = Get-Content -LiteralPath (Join-Path $root 'SCITER-RUNTIME.json') -Raw | ConvertFrom-Json
Get-VerifiedDownload "https://raw.githubusercontent.com/c-smile/sciter-sdk/$($runtime.commit)/$($runtime.path)" (Join-Path $downloads 'sciter.dll') $runtime.sha256
Get-VerifiedDownload 'https://raw.githubusercontent.com/c-smile/sciter-sdk/1c6b588cd090506d4b044dead06d7eae6e2235b7/license.htm' (Join-Path $downloads 'Sciter-license.htm') 'a52733cc90fb112a730b67bbbe6fc1102fad644753af9bd584cab8ba0da3db49'
Get-VerifiedDownload 'https://github.com/rustdesk-org/rdev/releases/download/usbmmidd_v2/usbmmidd_v2.zip' (Join-Path $downloads 'usbmmidd_v2.zip') '629b51e9944762bae73948171c65d09a79595cf4c771a82ebc003fbba5b24f51'
