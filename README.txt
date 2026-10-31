RuEkran universal Windows x86 corresponding source

This source builds one x86 portable EXE for 32-bit Windows and x64 via WOW64.
UI: upstream Sciter x86; backend protocol base: RustDesk 1.5.0.
Product version: release/version.json and source/res/ruekran-version*.

Follow the build-for-windows-sciter job in source/.github/workflows/flutter-build.yml.
Tools: nightly-2023-10-13-i686-pc-windows-msvc, 32-bit LLVM 15.0.6,
VS 2022 MSVC x86 tools, vcpkg x86-windows-static with the pinned manifest.
Sciter binding is pinned to dyn_x86 commit 674e07d3066ca9a92ced3816203ab6b652629d1e.
The exact native Sciter DLL revision and checksum are recorded in the delivery report.

Before using the supplied scripts, rename source/ to source-x86/ and build-scripts/
to scripts/. Keep release/ beside those directories. Scripts assume this workspace
layout and are intended to be adapted, not executed unchanged after extraction.
Adapt absolute F:/RuEkran paths throughout scripts/, including environment-x86.ps1,
build-universal.ps1 and archive-source.py, to your workspace. Install dependencies
and download the auxiliary runtime/driver files before packaging; the scripts do
not bootstrap a clean machine. SCITER-RUNTIME.json records the exact runtime.
Use build-scripts/build-universal.ps1 -Rebuild to reuse the included release number.
Use build-scripts/build-universal.ps1 without -Rebuild to reserve the next release.
Use package-universal.ps1 to package the application and sources into one EXE.
The portable packer and installer are the original RustDesk mechanisms.

Generated Sciter inline UI is included and can be regenerated with
PYTHONUTF8=1 python res/inline-sciter.py.
See LICENCE and third-party notices; ORIGINAL-BUILD-REPORT.md describes the older
x64 Flutter environment and is retained as historical context only.
