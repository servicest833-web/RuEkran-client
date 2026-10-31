"""Exercise the actual Windows update registry-command generator without changing HKLM."""
import pathlib
import subprocess
import sys
import tempfile
import textwrap

root = pathlib.Path(__file__).resolve().parent.parent
source = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else root / 'source-x86/src/platform/windows.rs').read_text(encoding='utf-8')
start = source.index('    fn get_reg_cmd(', source.index('pub fn update_me('))
end = source.index('\n    let reg_cmd =', start)
function = textwrap.dedent(source[start:end])
display_argument = '"0.1.0-alpha.2",' if 'display_version: &str' in function else ''
harness = function + '''
fn main() {
    let branded = get_reg_cmd("TEST", None, "icon", "1.5.0", DISPLAY
        "2026-10-21", "1", "5", "0", 100);
    assert!(branded.contains("/v DisplayVersion /t REG_SZ /d \\\"0.1.0-alpha.2\\\""));
    assert!(branded.contains("/v Version /t REG_SZ /d \\\"1.5.0\\\""));
    let stock = get_reg_cmd("TEST", Some(true), "icon", "1.5.0", STOCK
        "2026-10-21", "1", "5", "0", 100);
    assert!(stock.contains("/v DisplayVersion /t REG_SZ /d \\\"1.5.0\\\""));
    assert!(!stock.contains("/v DisplayIcon"));
}
'''.replace('DISPLAY', display_argument).replace('STOCK', '"1.5.0",' if display_argument else '')
with tempfile.TemporaryDirectory() as temporary:
    path = pathlib.Path(temporary)
    (path / 'test.rs').write_text(harness, encoding='utf-8')
    subprocess.run(['rustc', str(path / 'test.rs'), '-o', str(path / 'test.exe')], check=True)
    subprocess.run([str(path / 'test.exe')], check=True)
print('PASS: branded display version, technical version, and stock MSI registry behavior')
