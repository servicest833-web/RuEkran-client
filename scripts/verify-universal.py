"""Verify the universal x86 wrapper and every decompressed payload entry."""
import hashlib
import json
import pathlib
import struct
import sys
import zipfile
import brotli

root = pathlib.Path(__file__).resolve().parent.parent
version = json.loads((root / 'release/version.json').read_text())['display']
source = root / 'source-x86'
payload = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root / f'package/RuEkran-{version}-x86-delivery'
wrapper = pathlib.Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else root / 'dist/Windows/RuEkran-Windows-x86-x64.exe'
blob = (source / 'libs/portable/data.bin').read_bytes()
assert blob in wrapper.read_bytes(), 'Stale embedded payload'
marker = b'rustdesk'
assert blob.startswith(marker)
position = len(marker)
inventory = {}

def read(count):
    global position
    result = blob[position:position + count]
    assert len(result) == count
    position += count
    return result

while blob[position:position + len(marker)] != marker:
    name = read(struct.unpack('>I', read(4))[0]).decode('utf-8')
    compressed = read(struct.unpack('>I', read(4))[0])
    digest = read(32).decode('ascii')
    content = brotli.decompress(compressed)
    assert hashlib.md5(content).hexdigest() == digest, name
    relative = name.replace('\\', '/').removeprefix('./')
    assert content == (payload / relative).read_bytes(), name
    inventory[relative] = hashlib.sha256(content).hexdigest()
position += len(marker)
assert blob[position:].decode('utf-8').replace('\\', '/') == './RuEkran.exe'
assert set(inventory) == {p.relative_to(payload).as_posix() for p in payload.rglob('*') if p.is_file()}
assert 'rustdesk.exe' not in inventory
assert (payload / 'RuEkran.exe').read_bytes() == (source / 'target/release/rustdesk.exe').read_bytes()
client = (payload / 'RuEkran.exe').read_bytes()
assert b'if (type == "re-input-password") {\ntype = "input-password";\n}' in client.replace(b'\r\n', b'\n'), 'Missing password retry form preserving the error explanation'
assert b'function refreshPeerStatuses()' in client, 'Missing peer availability UI'
assert b'handler.query_peer_statuses(ids)' in client, 'Missing peer availability bridge'
assert b'ruekran-theme' in client and b'theme-switcher' in client, 'Missing main-screen theme switch'
assert b'handler.open_legal_file("license")' in client, 'Installer must open the included license'
assert b'view.open_url("http://rustdesk.com/privacy")' not in client, 'Stale installer agreement link'
assert b'<meta charset="utf-8"' in client, 'Missing explicit UTF-8 interface encoding'
assert b'<html window-resizable window-frame="extended">\n<head>\n<meta charset="utf-8"' in client, 'Remote session HTML must declare UTF-8'
assert b'MSK1 relay: msk1-relay.ruekran.ru' in client and b'TSK relay: 78.140.33.193' in client, 'Missing relay host captions'
assert b'var host = endpoint;' in client, 'Stale tooltip port/IPv6 formatter'
assert b'RuEkran relay probe:' in client, 'Missing measured automatic relay selection'
assert '<div>Установить RuЭкран</div>'.encode('utf-8') in client, 'Missing portable install prompt'
assert 'Portable-режим. Права администратора получены.'.encode('utf-8') not in client, 'Stale portable privileges notice'
assert b'handler.ruekran_update("install")' in client, 'Missing in-client update action'
assert b'Rust&#68;esk' in client, 'About attribution is vulnerable to brand-name substitution'
assert "Лицензии и сведения о программе".encode('utf-8') in client, 'Stale About legal link'
assert "Исходный код этой сборки".encode('utf-8') not in client, 'Stale local source-folder link'
assert b'padding: 12px 20px;' in client, 'Stale install card spacing'
assert (payload / 'RuEkran-NOTICE.txt').read_bytes().startswith(b'\xef\xbb\xbf'), 'Win7 Notepad needs a UTF-8 BOM'
assert (payload / 'sciter.dll').read_bytes() == (root / 'downloads/sciter-x86.dll').read_bytes()
for relative in ['res/icon.ico', 'res/tray-icon.ico']:
    assert (source / relative).read_bytes() == (source / 'flutter/assets/icon.ico').read_bytes()
for path in [wrapper, payload / 'RuEkran.exe', payload / 'sciter.dll']:
    data = path.read_bytes()
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    assert data[pe:pe + 4] == b'PE\0\0'
    assert struct.unpack_from('<H', data, pe + 4)[0] == 0x14c, f'Expected x86: {path}'
    assert struct.unpack_from('<H', data, pe + 24)[0] == 0x10b, f'Expected PE32: {path}'
assert 'usbmmidd_v2/deviceinstaller64.exe' in inventory
with zipfile.ZipFile(payload / 'source/RuEkran-source.zip') as archive:
    assert archive.testzip() is None
    assert archive.read('source/res/ruekran-version.txt').decode().strip() == version
    assert 'source/src/ui/inline.rs' in archive.namelist()
    assert 'source/src/ui/peer_status.rs' in archive.namelist()
    assert archive.read('source/res/admin-public-key.bin') == (source / 'res/admin-public-key.bin').read_bytes()
    assert b'apply_service_ticket' in archive.read('source/src/platform/windows/admin_id.rs')
    assert 'handler.get_app_name() == "RuЭкран"'.encode() in archive.read('source/src/ui/index.tis')
    assert 'source/res/ruekran.png' in archive.namelist()
    assert 'build-scripts/package-universal.ps1' in archive.namelist()
    assert b'handler.open_legal_file("license")' in archive.read('source/src/ui/install.tis')
    assert b'RuEkran' in archive.read('source/libs/hbb_common/src/config.rs')
(root / 'logs/universal-payload-sha256.json').write_text(json.dumps(inventory, indent=2))
print(f'PASS: {len(inventory)} files match staging; wrapper/client/Sciter are PE32 x86; source ZIP valid; version {version}.')
