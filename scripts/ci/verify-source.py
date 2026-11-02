"""Check the imported immutable release snapshot before running build tools."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2]
manifest = json.loads((root / 'SOURCE-MANIFEST.json').read_text(encoding='utf-8'))
version = json.loads((root / 'release/version.json').read_text(encoding='utf-8-sig'))
assert manifest['release'] == version['display'] == '0.1.0-beta.2'
assert manifest['windows_build'] == version['build'] == 24
assert manifest['published_source_archive_sha256'] == '220672961b9d5cd234155187339abaffff154f4bad866a42febe1841708a29c4'
for relative, expected in manifest['files'].items():
    path = root / relative
    assert path.resolve().is_relative_to(root.resolve()), relative
    assert path.is_file(), f'Missing source: {relative}'
    assert hashlib.sha256(path.read_bytes()).hexdigest() == expected, f'Source checksum mismatch: {relative}'
assert (root / 'source-x86/res/ruekran-version.txt').read_text(encoding='utf-8').strip() == version['display']
assert (root / 'source-x86/res/ruekran-version-numeric.txt').read_text(encoding='utf-8').strip() == version['numeric']
assert (root / 'source-x86/libs/hbb_common/Cargo.toml').is_file()
print(f"PASS: {len(manifest['files'])} files match the published Beta 2 source archive; version/build preserved.")
