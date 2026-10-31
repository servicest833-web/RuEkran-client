$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot
Import-Module 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\Microsoft.VisualStudio.DevShell.dll'
Enter-VsDevShell -VsInstallPath 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools' -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64'
$env:VCPKG_ROOT = "$workspace\tools\vcpkg"
$env:LIBCLANG_PATH = "$workspace\tools\llvm-15\bin"
$env:LLVM_HOME = "$workspace\tools\llvm-15"
$env:RUSTUP_TOOLCHAIN = '1.75.0-x86_64-pc-windows-msvc'
$env:Path = "$workspace\tools\cmake-4.3.0-windows-x86_64\bin;$workspace\tools\flutter\bin;$workspace\tools\llvm-15\bin;$workspace\tools\bin;" + $env:Path
