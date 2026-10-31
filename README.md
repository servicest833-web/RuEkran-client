# РуЭкран — Windows client

Development branch **next**: contains the About/legal-text correction, compiled locally but not published as a new executable. For the exact published **0.1.0-alpha.19** Windows source (build **21**), use tag [`v0.1.0-alpha.19`](https://github.com/servicest833-web/RuEkran-client/tree/v0.1.0-alpha.19).

РуЭкран — Windows-клиент удалённого доступа на основе RustDesk. Этот репозиторий содержит фактические исходники выпущенной Windows-версии: изменения РуЭкран, ресурсы, общий модуль `hbb_common` и скрипты сборки. Это изменённая сборка, а не официальный выпуск RustDesk.

## Downloads and source

- Product website: https://ruekran.ru/
- Windows Alpha 19: https://ruekran.ru/releases/RuEkran-Windows-x86-x64-21.exe
- Corresponding source archive: https://ruekran.ru/releases/RuEkran-source-21.zip
- Privacy: https://ruekran.ru/privacy/
- Terms of use: https://ruekran.ru/terms/

`SOURCE-MANIFEST.json` records the SHA-256 of the published source archive and every Windows source file imported from it. On tag `v0.1.0-alpha.19`, the `source-x86/` files match that archive byte for byte. The manifest describes that release tag, not subsequent development changes. `SOURCE-REVISIONS.json` and `changes/` preserve the recorded source revisions and the release-time changes.

## Contents

- `source-x86/` — Windows source tree, including the actual modified `libs/hbb_common` sources. It is already present; a submodule checkout is not needed for this snapshot.
- `scripts/` — Windows build, packaging and verification scripts.
- `release/` — version and release procedure documentation recorded with this release.
- `SCITER-RUNTIME.json` — revision and SHA-256 of the external Sciter runtime.

The separate Java Android application, server administration code, production deployment configuration and private signing credentials are not included. Cross-platform files inherited from the RustDesk source tree are retained as part of that tree; they are not the separate РуЭкран Java Android application.

## Build

The published executable uses the Sciter x86 interface. The x86 binary also runs on supported x64 Windows through WOW64. This repository does not assert a fresh clean-machine build has been independently reproduced.

Use Visual Studio 2022 C++ x86 tools, `nightly-2023-10-13-i686-pc-windows-msvc`, LLVM 15.0.6 x86, Python, and the pinned native dependencies in `source-x86/vcpkg.json` and `source-x86/vcpkg-configuration.json`. The original Windows build workflow is retained under `source-x86/.github/workflows/` as reference, not as an enabled root-level GitHub workflow.

1. Install the tools and dependencies. Adjust the machine-specific Visual Studio paths in `scripts/environment*.ps1`. The scripts expect local tools under `tools/` and native libraries under `tools/vcpkg-x86-installed/`.
2. From the repository root, run `scripts/build-universal.ps1 -Rebuild` to retain the included release number.
3. Before packaging, obtain the exact Sciter DLL described by `SCITER-RUNTIME.json`, its licence, and the auxiliary `usbmmidd_v2` driver distribution expected by `scripts/package-universal.ps1`. These external binary dependencies are not supplied by this repository.
4. Run `scripts/package-universal.ps1`, then `python scripts/verify-universal.py`.

The generated Sciter interface is included and can be regenerated from `source-x86/` with `python res/inline-sciter.py`. The source includes the public verification keys needed by the client. No production private key is required to compile it. Publishing a new update accepted by existing clients requires the operator's separate private signing key, which is not distributed.

See `README.txt` for the release-time environment notes. Packaging source archives does not grant access to the production update-signing service.

## Licence and attribution

The covered client source and its modifications are distributed under **GNU Affero General Public License, version 3**, subject to the notices in the individual files. See [LICENSE](LICENSE), [upstream licence](source-x86/LICENCE) and [release notices](source-x86/flutter/assets/RuEkran-NOTICE.txt). Third-party components retain their own licences.

RustDesk upstream: https://github.com/rustdesk/rustdesk. Existing copyright notices, including Purslane Tech Pte. Ltd. and other contributors, are retained. РуЭкран is provided by its own operator; upstream authors are not represented as the provider of this modified release.

The software is provided without warranty to the extent permitted by the licence and applicable law. The AGPL rights to use, study, modify and redistribute covered source are not restricted by the service's terms.
