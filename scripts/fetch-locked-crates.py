import concurrent.futures, hashlib, os, pathlib, tomllib, urllib.request, sys
lock = tomllib.loads(pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else r'D:\codex\RuEkran\source\Cargo.lock').read_text(encoding='utf-8'))
cache = pathlib.Path.home() / '.cargo/registry/cache' / (sys.argv[2] if len(sys.argv) > 2 else 'index.crates.io-6f17d22bba15001f')
cache.mkdir(parents=True, exist_ok=True)
missing = [p for p in lock['package'] if p.get('source', '').startswith('registry+') and not (cache / f"{p['name']}-{p['version']}.crate").exists()]
print(f'Missing registry packages: {len(missing)}', flush=True)
def fetch(p):
    name, version = p['name'], p['version']
    url = f'https://static.crates.io/crates/{name}/{name}-{version}.crate'
    data = urllib.request.urlopen(url, timeout=60).read()
    if hashlib.sha256(data).hexdigest() != p['checksum']:
        raise RuntimeError(f'Checksum mismatch: {name}-{version}')
    (cache / f'{name}-{version}.crate').write_bytes(data)
    return f'{name}-{version}: verified'
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
    for result in pool.map(fetch, missing):
        print(result, flush=True)
