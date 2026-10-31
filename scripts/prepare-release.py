"""Reserve one RuEkran release number; retries reuse the generated files."""
import argparse
import json
import pathlib
import re

workspace = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--base')
parser.add_argument('--channel', choices=['alpha', 'beta', 'rc', 'stable'])
parser.add_argument('--reuse', action='store_true')
args = parser.parse_args()
state_file = workspace / 'release/version.json'
old = json.loads(state_file.read_text()) if state_file.exists() else {}
explicit_base = args.base
explicit_channel = args.channel
args.base = args.base or old.get('base', '0.1.0')
args.channel = args.channel or old.get('channel', 'alpha')
if not re.fullmatch(r'\d+\.\d+\.\d+', args.base):
    parser.error('Base version must have three numeric components.')
if args.reuse:
    if not old:
        parser.error('No release has been reserved yet.')
    state = old
else:
    if not explicit_channel and args.base == old.get('base') and old.get('sequence', 0) >= 20:
        args.channel = {'alpha': 'beta', 'beta': 'stable'}.get(args.channel, args.channel)
    if args.channel == 'stable' and old.get('channel') == 'stable' and args.base == old.get('base'):
        if explicit_base:
            parser.error('This stable version already exists. Use a new base or --reuse.')
        major, minor, patch = map(int, args.base.split('.'))
        args.base = f'{major}.{minor}.{patch + 1}'
    sequence = old.get('sequence', 0) + 1 if (old.get('base'), old.get('channel')) == (args.base, args.channel) else 1
    if args.channel in ('alpha', 'beta') and sequence > 20:
        parser.error('Alpha and beta end at 20. Select the next channel or use --reuse.')
    counter = old.get('build', 0) + 1
    display = args.base if args.channel == 'stable' else f'{args.base}-{args.channel}.{sequence}'
    numeric = f'{args.base}.{counter}'
    if any(int(part) > 65535 for part in numeric.split('.')):
        parser.error('Windows version components must be at most 65535.')
    state = dict(base=args.base, channel=args.channel, sequence=sequence,
                 build=counter, display=display, numeric=numeric,
                 upstream='1.5.0')
for source_name in ['source', 'source-x86']:
    res = workspace / source_name / 'res'
    if not res.exists():
        continue
    (res / 'ruekran-version.txt').write_text(state['display'] + '\n', encoding='utf-8')
    (res / 'ruekran-version-numeric.txt').write_text(state['numeric'] + '\n', encoding='utf-8')
    (res / 'ruekran-version.h').write_text(
        '#define RUEKRAN_VERSION_NUMBER ' + state['numeric'].replace('.', ',') + '\n'
        '#define RUEKRAN_VERSION_TEXT "' + state['display'] + '"\n', encoding='ascii')
state_file.parent.mkdir(exist_ok=True)
temporary = state_file.with_suffix('.tmp')
temporary.write_text(json.dumps(state, indent=2) + '\n', encoding='utf-8')
temporary.replace(state_file)
print(json.dumps(state))
