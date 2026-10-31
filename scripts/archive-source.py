"""Package the actual source tree, including its pinned Git submodules."""
import json
import hashlib
import pathlib
import subprocess
import sys
import zipfile

workspace = pathlib.Path(__file__).resolve().parent.parent
root = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else workspace / 'source')
destination = pathlib.Path(sys.argv[1])
destination.parent.mkdir(parents=True, exist_ok=True)
metadata = {}

def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args])

def add_repo(archive, repo, prefix):
    metadata[prefix or '.'] = git(repo, 'rev-parse', 'HEAD').decode().strip()
    paths = git(repo, 'ls-files', '-z').decode('utf-8').split('\0')
    for name in paths:
        if not name:
            continue
        path = repo / name
        archive_path = 'source/' + prefix + name
        if path.is_file():
            archive.write(path, archive_path)
        elif path.is_dir() and (path / '.git').exists():
            add_repo(archive, path, prefix + name + '/')
    archive.writestr('changes/' + (prefix.replace('/', '_') or 'root') + '.patch',
                     git(repo, 'diff', '--binary', 'HEAD'))

with zipfile.ZipFile(destination, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
    add_repo(archive, root, '')
    for name in ['ruekran_icon.png', 'icon.png', 'icon.ico', 'AGPL-3.0.txt', 'RuEkran-NOTICE.txt']:
        path = root / 'flutter' / 'assets' / name
        arcname = 'source/flutter/assets/' + name
        if arcname not in archive.namelist():
            archive.write(path, arcname)
    for relative in ['src/bridge_generated.rs', 'src/bridge_generated.io.rs',
                     'flutter/lib/generated_bridge.dart',
                     'flutter/lib/generated_bridge.freezed.dart',
                     'flutter/macos/Runner/bridge_generated.h',
                     'flutter/ios/Runner/bridge_generated.h',
                     'src/ui/inline.rs', 'src/ui/peer_status.rs',
                     'src/client/relay_region.rs', 'examples/relay_audit.rs',
                     'src/platform/windows_text_focus.rs', 'examples/text_focus_audit.rs',
                     'libs/portable/app_metadata.toml',
                     'src/platform/windows/admin_id.rs', 'res/admin-public-key.bin', 'res/ruekran.png',
                     'src/platform/windows/ruekran_usage.rs', 'res/portable-manifest.xml',
                     'libs/portable/src/ui.rs', 'libs/portable/src/bin_reader.rs',
                     'libs/portable/src/res/label.png', 'libs/portable/src/res/spin.gif',
                     'src/platform/windows/ruekran_update.rs', 'res/update-public-key.bin',
                     'res/ruekran-build-version.rs', 'res/ruekran-version.txt',
                     'res/ruekran-version-numeric.txt', 'res/ruekran-version.h']:
        if (root / relative).is_file() and 'source/' + relative not in archive.namelist():
            archive.write(root / relative, 'source/' + relative)
    for path in (workspace / 'scripts').iterdir():
        if path.is_file():
            archive.write(path, 'build-scripts/' + path.name)
    report = workspace / 'dist/BUILD-REPORT.md'
    if report.exists():
        archive.write(report, 'ORIGINAL-BUILD-REPORT.md')
    archive.writestr('SOURCE-REVISIONS.json', json.dumps(metadata, indent=2))
    release_state = workspace / 'release/version.json'
    if release_state.exists():
        archive.write(release_state, 'release/version.json')
        archive.write(release_state.with_name('VERSIONING.md'), 'release/VERSIONING.md')
        archive.write(release_state.with_name('CHANGELOG.md'), 'release/CHANGELOG.md')
        if release_state.with_name('UPDATES.md').exists():
            archive.write(release_state.with_name('UPDATES.md'), 'release/UPDATES.md')
    if root.name == 'source-x86':
        runtime = workspace / 'downloads/sciter-x86.dll'
        archive.writestr('SCITER-RUNTIME.json', json.dumps({
            'repository': 'https://github.com/c-smile/sciter-sdk',
            'commit': runtime.with_name('sciter-x86-commit.txt').read_text().strip(),
            'path': 'bin.win/x32/sciter.dll',
            'sha256': hashlib.sha256(runtime.read_bytes()).hexdigest(),
        }, indent=2))
        archive.writestr('README.txt', '''RuEkran universal Windows x86 corresponding source

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
''')
    archive.writestr('README-FLUTTER.txt' if root.name == 'source-x86' else 'README.txt', '''RuEkran corresponding source, 2026-10-18

source/ contains the actual modified source tree with its hbb_common submodule.
changes/ records changes against the exact upstream revisions in SOURCE-REVISIONS.json.
New branding/legal assets are included. No private server key is required or included.

Build on Windows x64 with Rust 1.75.0, Flutter 3.24.5, LLVM 15.0.6,
Visual Studio 2022 C++ tools and the vcpkg baseline in source/vcpkg.json.
See ORIGINAL-BUILD-REPORT.md and build-scripts/environment.ps1 for exact tool paths
and pinned dependency details. Adapt local F:/RuEkran paths to your workspace.
Generated Rust/Dart bridges from this build are included. To regenerate them,
install flutter_rust_bridge_codegen 1.80.1 and cargo-expand 1.0.95 and run
build-scripts/generate-bridge.ps1. Run flutter pub get in source/flutter.
Run build-scripts/build-client.ps1, then package-branded.ps1.
The packaging script takes verified auxiliary drivers from the previous package;
their original download names and SHA256 checks are in package-client.ps1.
For a fresh workspace, stage those auxiliary files following that script first.
The final application uses upstream RustDesk portable packing and installer.

Main executable is RuEkran.exe. Public display name is RuEkран / RuЭкран.
Normal uninstalled Windows startup requests the upstream portable elevated helper.
Installed/service/management argument paths retain their upstream behavior.
The RustDesk AGPL license and third-party notices remain in source/.
''')
print(f'{destination}: {destination.stat().st_size} bytes; {len(metadata)} repositories')
