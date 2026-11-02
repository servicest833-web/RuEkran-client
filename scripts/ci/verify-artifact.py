"""Validate the built package rather than treating a successful compiler exit as proof."""
import hashlib
import json
from pathlib import Path
import subprocess
import pefile

root = Path(__file__).resolve().parents[2]
out = root / 'dist/ci-windows'
version = json.loads((root / 'release/version.json').read_text(encoding='utf-8-sig'))
pe = pefile.PE(str(out / 'RuEkran.exe'))
assert pe.FILE_HEADER.Machine == 0x14c and pe.OPTIONAL_HEADER.Magic == 0x10b
strings = {k.decode(): v.decode() for entry in pe.FileInfo for info in entry if hasattr(info, 'StringTable') for table in info.StringTable for k, v in table.entries.items()}
assert strings['ProductVersion'] == version['display'], strings
assert strings['FileVersion'] == version['numeric'], strings
assert 'Ru' in strings['ProductName']
assert pe.OPTIONAL_HEADER.DATA_DIRECTORY[4].Size == 0, 'Unexpected certificate in unsigned package'
runtime = json.loads((root / 'SCITER-RUNTIME.json').read_text(encoding='utf-8-sig'))
assert hashlib.sha256((out / 'sciter.dll').read_bytes()).hexdigest() == runtime['sha256']
assert (out / 'usbmmidd_v2/deviceinstaller64.exe').is_file()
assert (out / 'AGPL-3.0.txt').read_bytes() == (root / 'source-x86/LICENCE').read_bytes()
assert 'Sciter Engine' in (out / 'README.txt').read_text(encoding='utf-8-sig')
manifest_paths = set()
for line in (out / 'SHA256SUMS.txt').read_text(encoding='ascii').splitlines():
    expected, relative = line.split('  ', 1)
    path = out / relative
    assert path.resolve().is_relative_to(out.resolve()), relative
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected, relative
    manifest_paths.add(relative)
assert manifest_paths == {p.relative_to(out).as_posix() for p in out.rglob('*') if p.is_file() and p.name != 'SHA256SUMS.txt'}
provenance = json.loads((out / 'BUILD-PROVENANCE.json').read_text(encoding='utf-8-sig'))
assert provenance['commit'] == subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
assert provenance['version'] == version['display'] and provenance['build'] == version['build']
assert provenance['unsigned'] and not provenance['public_release']
print(f"PASS: PE32, release {version['display']}, build {version['build']}, pinned runtime, driver, licence and {len(manifest_paths)} file hashes.")
