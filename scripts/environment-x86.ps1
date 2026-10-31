$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\environment.ps1"
$workspace = Split-Path $PSScriptRoot
Enter-VsDevShell -VsInstallPath 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools' -SkipAutomaticLocation -DevCmdArguments '-arch=x86 -host_arch=x64'
$env:VCPKG_ROOT = "$workspace\tools\vcpkg"
$env:RUSTUP_TOOLCHAIN = 'nightly-2023-10-13-i686-pc-windows-msvc'
$env:LIBCLANG_PATH = "$workspace\tools\llvm-15-x86\bin"
$env:LLVM_HOME = "$workspace\tools\llvm-15-x86"
$env:VCPKG_INSTALLED_ROOT = "$workspace\tools\vcpkg-x86-installed"
$legacyNativePath = Join-Path $env:VCPKG_ROOT 'installed\x86-windows-static'
$nativePath = Join-Path $env:VCPKG_INSTALLED_ROOT 'x86-windows-static'
if (-not (Test-Path -LiteralPath $nativePath)) { throw 'Install x86 native dependencies first.' }
if (-not (Test-Path -LiteralPath $legacyNativePath)) {
    New-Item -ItemType Junction -Path $legacyNativePath -Target $nativePath | Out-Null
}
$env:VCPKG_DEFAULT_TRIPLET = 'x86-windows-static'
$env:VCPKG_DEFAULT_HOST_TRIPLET = 'x86-windows-static'
$env:PYTHONUTF8 = '1'
$env:Path = "$workspace\tools\llvm-15-x86\bin;" + $env:Path
