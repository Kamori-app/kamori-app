#!/usr/bin/env python3
"""Focused real-daemon safety controls. Uses only the reserved network/tools name."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
PODMAN = '/opt/podman/bin/podman'
SOCKET = 'unix:///private/tmp/podman/podman-machine-default-api.sock'
NAMESPACE = 'kamori-readiness-restore-20261015'
LABELS = {'kamori.readiness': '20261015', 'kamori.readiness.task': 'restore'}
BASE = Path('/private/tmp/kamori-readiness-20261015/restore')
SELECTORS = ('CONTAINER_HOST', 'CONTAINER_CONNECTION', 'CONTAINER_SSHKEY',
             'DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH')


def control(*args):
    env = {k: v for k, v in os.environ.items() if k not in SELECTORS}
    return subprocess.run([PODMAN, '--url', SOCKET, *args], env=env, check=True,
                          text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def load_runner(environment):
    # Even a regressed runner must never inherit a real remote endpoint in these tests.
    local_env = {k: v for k, v in os.environ.items() if k not in SELECTORS and k != 'RESTORE_ENGINE'}
    local_env['CONTAINER_HOST'] = SOCKET
    local_env.update(environment)
    with patch.dict(os.environ, local_env, clear=True):
        spec = importlib.util.spec_from_file_location('restore_control_target', HERE / 'run.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
    return module


class EndpointControls(unittest.TestCase):
    def test_fixed_endpoint_ignores_conflicting_selectors(self):
        # All conflicting destinations are nonexistent LOCAL resources, never remote services.
        cases = [
            {'CONTAINER_HOST': 'unix:///private/tmp/kamori-review-no-daemon.sock'},
            {'CONTAINER_CONNECTION': 'kamori-no-such-local-connection'},
            {'DOCKER_HOST': 'unix:///private/tmp/kamori-review-no-docker.sock',
             'DOCKER_CONTEXT': 'kamori-no-such-local-context'},
        ]
        cases.append({key: value for case in cases for key, value in case.items()})
        for index, env in enumerate(cases):
            with self.subTest(selectors=env):
                runner = load_runner(env)
                with (ARTIFACTS / f'endpoint-{index}.log').open('w') as log:
                    runner.log = log
                    result = runner.docker('version', '--format', '{{.Server.Version}}')
                    self.assertEqual(result.stdout.strip(), '5.8.2')
                    for key in SELECTORS:
                        self.assertTrue(key not in runner.ENV, f'endpoint selector leaked: {key}')
                    self.assertIn('--url ' + SOCKET, Path(log.name).read_text())

    def test_arbitrary_engine_override_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, 'unsupported RESTORE_ENGINE'):
            load_runner({'RESTORE_ENGINE': '/usr/bin/false'})


class FailedStartControls(unittest.TestCase):
    def setUp(self):
        # Lock shared with the full runner. A foreign invocation must never be cleaned up.
        self.lock = BASE / '.running'
        self.lock.mkdir()
        self.network_created = False
        self.name = NAMESPACE + '-tools'
        self.runner = None
        try:
            names = control('container', 'ls', '-a', '--format', '{{.Names}}').stdout.splitlines()
            self.assertNotIn(self.name, names, 'reserved container collision')
            networks = control('network', 'ls', '--format', '{{.Name}}').stdout.splitlines()
            self.assertNotIn(NAMESPACE, networks, 'reserved network collision')
            control('network', 'create', '--label', 'kamori.readiness=20261015',
                    '--label', 'kamori.readiness.task=restore', NAMESPACE)
            self.network_created = True
            # Baseline M1 reproduction is explicitly local even before I1 is fixed.
            self.runner = load_runner({'CONTAINER_HOST': SOCKET})
            self.runner.RUN = ARTIFACTS / self._testMethodName
            self.runner.RUN.mkdir()
            self.runner.log = (self.runner.RUN / 'commands.log').open('w')
            self.runner.created = [('network', NAMESPACE)]
        except BaseException:
            self.tearDown()
            raise

    def tearDown(self):
        # Independent scoped cleanup also runs on RED; inspect exact labels before removal.
        try:
            if self.runner is not None:
                self.runner.log.close()
            if self.network_created:
                names = control('container', 'ls', '-a', '--format', '{{.Names}}').stdout.splitlines()
                if self.name in names:
                    record = json.loads(control('container', 'inspect', self.name).stdout)[0]
                    self.assertTrue(all(record['Config']['Labels'].get(k) == v for k, v in LABELS.items()))
                    control('container', 'rm', '-f', self.name)
                networks = control('network', 'ls', '--format', '{{.Name}}').stdout.splitlines()
                if NAMESPACE in networks:
                    record = json.loads(control('network', 'inspect', NAMESPACE).stdout)[0]
                    labels = record.get('Labels', record.get('labels'))
                    self.assertTrue(all(labels.get(k) == v for k, v in LABELS.items()))
                    control('network', 'rm', NAMESPACE)
        finally:
            self.lock.rmdir()

    def test_failed_start_is_removed_and_error_propagates(self):
        with self.assertRaisesRegex(RuntimeError, 'command exited 127'):
            self.runner.start('tools', args=('/exercise/no-such-executable',))
        record = json.loads(control('container', 'inspect', self.name).stdout)[0]
        self.assertEqual(record['State']['Status'], 'created')
        try:
            self.runner.cleanup()
        finally:
            inventory = control('container', 'ls', '-a', '--format', '{{.Names}}').stdout.splitlines()
            (self.runner.RUN / 'independent-inventory.json').write_text(json.dumps(inventory))
        self.assertNotIn(self.name, inventory, 'failed-start container leaked despite cleanup')
        cleanup = json.loads((self.runner.RUN / 'cleanup.json').read_text())
        self.assertEqual(cleanup['errors'], [])
        self.assertFalse(any(cleanup['remaining'].values()))


    def test_failure_before_creation_has_clean_inventory(self):
        with self.assertRaisesRegex(RuntimeError, 'command exited 125'):
            self.runner.start('tools', env=self.runner.RUN / 'absent.env')
        self.runner.cleanup()
        self.assertNotIn(self.name, control('container', 'ls', '-a', '--format', '{{.Names}}').stdout.splitlines())
        cleanup = json.loads((self.runner.RUN / 'cleanup.json').read_text())
        self.assertEqual(cleanup['errors'], [])
        self.assertFalse(any(cleanup['remaining'].values()))

    def test_failed_start_cleanup_still_requires_matching_labels(self):
        with self.assertRaisesRegex(RuntimeError, 'command exited 127'):
            self.runner.start('tools', args=('/exercise/no-such-executable',))
        # The fixture remains correctly labeled. Change the expected ownership
        # contract to exercise refusal without creating an unlabeled resource.
        self.runner.LABELS = dict(LABELS, **{'kamori.readiness.task': 'mismatched-test-owner'})
        with self.assertRaisesRegex(RuntimeError, 'cleanup incomplete'):
            self.runner.cleanup()
        self.assertIn(self.name, control('container', 'ls', '-a', '--format', '{{.Names}}').stdout.splitlines())
        denied = json.loads((self.runner.RUN / 'cleanup.json').read_text())
        self.assertIn(self.name, denied['remaining']['container'])
        self.assertTrue(any('FOREIGN RESOURCE' in error for error in denied['errors']))
        (self.runner.RUN / 'cleanup-denied.json').write_text(json.dumps(denied, indent=2))
        self.runner.LABELS = LABELS.copy()
        self.runner.cleanup()
        self.assertNotIn(self.name, control('container', 'ls', '-a', '--format', '{{.Names}}').stdout.splitlines())


if __name__ == '__main__':
    os.umask(0o077)
    BASE.mkdir(parents=True, exist_ok=True)
    ARTIFACTS = Path(tempfile.mkdtemp(prefix='controls-', dir=BASE))
    print(f'Artifacts: {ARTIFACTS}', flush=True)
    unittest.main(verbosity=2)
