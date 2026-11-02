# Windows Beta 2 build

## GitHub Actions

1. Open https://github.com/servicest833-web/RuEkran-client/actions.
2. Select **Windows build (manual, no release)**.
3. Click **Run workflow**, choose `main`, and confirm **Run workflow**.
4. Inspect each step. The first clean native dependency build can take a long time;
   the job has a 240 minute limit and caches pinned tool downloads/native packages.
5. Only a successful run creates `RuEkran-Windows-beta2-unsigned-<run>-<attempt>`.
   Download it from **Artifacts** at the bottom of that run. GitHub login is needed.
6. Unpack the ZIP and launch `RuEkran.exe` with all companion files beside it.

No payment card, production key, website password or SignPath token is required
by this workflow. GitHub can still require account verification or an Actions
policy change; these account decisions belong to the repository owner.

The workflow requests `contents: read`, has no push/tag/scheduled trigger and has
no deployment or release creation step. Every build is requested manually.
Artifacts expire after seven days. This is a build artifact, not a GitHub Release.

## What is built

- Source: the checked-out commit's imported Beta 2 snapshot, validated against
  `SOURCE-MANIFEST.json` before generated files are changed.
- Product version: `0.1.0-beta.2`; Windows version `0.1.0.24`; no version increment.
- Target/compiler host: `nightly-2023-10-13-i686-pc-windows-msvc`.
- Native libraries: the vcpkg baseline/overlays declared by the published source,
  triplet and host triplet `x86-windows-static`.
- LLVM: official 15.0.6 x86 installer, checked by SHA-256.
- Client features: `inline,vram,hwcodec`; same release compilation switches.
- Sciter: exact external revision/hash from `SCITER-RUNTIME.json`.
- Driver: upstream `usbmmidd_v2` release archive, checked by SHA-256.

The package contains the real freshly compiled client, companion runtime, driver,
licences, `BUILD-PROVENANCE.json`, and a complete `SHA256SUMS.txt`. Validation checks
PE32 architecture, version resources, pinned runtime and all output hashes.
This does not replace a real connection/install test on physical computers.

The output is an unpacked unsigned package. It is not the public self-extracting
installer. Rebuilding that installer and releasing it are separate operations;
published update files/hashes must not be overwritten by a CI rebuild.

## Local use

Use a separate clean checkout on Windows with VS 2022 C++ x86 tools, rustup,
Git and Python 3.11. From the repository root:

```powershell
python scripts/ci/verify-source.py
./scripts/ci/setup-windows.ps1
./scripts/ci/build-windows.ps1
python scripts/ci/verify-artifact.py
```

For local use, run setup and build in the same PowerShell session so the
environment is retained. The scripts install downloaded LLVM only into `.ci/`;
the workflow is intended primarily for disposable GitHub-hosted runners.
Never place signing keys or production credentials in this checkout.

## SignPath is a separate approval

A successful GitHub Actions run allows the application to accurately name GitHub
Actions as its build system. It does not grant a certificate. Foundation's
proprietary-component restriction must be clarified for Sciter first:
https://signpath.org/terms.html

Do not claim full compliance or add a SignPath sponsorship statement before
acceptance. If accepted, install the official GitHub App for this repository only,
set up the requested project/policy and add a dedicated submitter credential in a
protected GitHub Environment. Require a human signing approver. None of these
credentials or permissions are configured by the current build-only workflow.

Integration documentation: https://docs.signpath.io/trusted-build-systems/github
