import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

class ReleaseVersionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = pathlib.Path(self.temporary.name)
        (self.root / 'scripts').mkdir()
        self.script = self.root / 'scripts/prepare-release.py'
        shutil.copyfile(pathlib.Path(__file__).with_name('prepare-release.py'), self.script)
        for source in ['source', 'source-x86']:
            (self.root / source / 'res').mkdir(parents=True)

    def reserve(self, *args):
        return json.loads(subprocess.check_output([sys.executable, str(self.script), *args]))

    def test_new_releases_increment_and_rebuilds_keep_the_release_number(self):
        first = self.reserve()
        self.assertEqual(first['display'], '0.1.0-alpha.1')
        self.assertEqual(self.reserve('--reuse'), first)
        second = self.reserve()
        self.assertEqual(second['display'], '0.1.0-alpha.2')
        for source in ['source', 'source-x86']:
            self.assertEqual((self.root / source / 'res/ruekran-version.txt').read_text().strip(), second['display'])
            self.assertEqual((self.root / source / 'res/ruekran-version-numeric.txt').read_text().strip(), '0.1.0.2')

    def test_channel_changes_reset_prerelease_sequence_and_keep_builds_monotonic(self):
        self.reserve()
        beta = self.reserve('--channel', 'beta')
        self.assertEqual((beta['display'], beta['build']), ('0.1.0-beta.1', 2))
        next_beta = self.reserve()
        self.assertEqual((next_beta['display'], next_beta['build']), ('0.1.0-beta.2', 3))
        next_base = self.reserve('--base', '0.2.0', '--channel', 'alpha')
        self.assertEqual((next_base['display'], next_base['numeric']), ('0.2.0-alpha.1', '0.2.0.4'))

    def test_new_stable_builds_increment_patch_instead_of_reusing_a_public_version(self):
        self.assertEqual(self.reserve('--channel', 'stable')['display'], '0.1.0')
        self.assertEqual(self.reserve()['display'], '0.1.1')
        self.assertEqual(self.reserve('--reuse')['display'], '0.1.1')

    def test_alpha_20_then_beta_20_then_stable_keep_global_build_increasing(self):
        for number in range(1, 21):
            state = self.reserve()
            self.assertEqual(state['display'], f'0.1.0-alpha.{number}')
        self.assertEqual(self.reserve('--reuse'), state)
        for number in range(1, 21):
            state = self.reserve()
            self.assertEqual((state['display'], state['build']), (f'0.1.0-beta.{number}', number + 20))
        self.assertEqual(self.reserve('--reuse'), state)
        self.assertEqual((self.reserve()['display'], self.reserve('--reuse')['build']), ('0.1.0', 41))

if __name__ == '__main__':
    unittest.main()
