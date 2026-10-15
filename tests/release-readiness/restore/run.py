#!/usr/bin/env python3
"""Local-only destructive recovery exercise; every mutable resource is explicitly owned."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
NAMESPACE = 'kamori-readiness-restore-20261015'
IMAGE = 'localhost/kamori-readiness-restore:20261015'
MIGRATOR = os.environ.get('RESTORE_MIGRATOR_IMAGE', '2fa21cae60f4da730ca8c248612c453e56100d59fa33b6afa8cc3790f89b2981')
MINIO = 'quay.io/minio/minio:RELEASE.2025-09-07T16-13-09Z@sha256:14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e'
ENGINE = os.environ.get('RESTORE_ENGINE', '/Users/yujin/.local/bin/docker')
# Refuse remote engines: this harness never targets a production service.
SOCKET = 'unix:///private/tmp/podman/podman-machine-default-api.sock'
ENV = dict(os.environ, DOCKER_HOST=SOCKET)
LABELS = {'kamori.readiness': '20261015', 'kamori.readiness.task': 'restore'}
ROLES = ('pg-source', 'pg-recovered', 'blob-primary', 'blob-dr', 'tools')
VOLUMES = ('pg-source-data', 'pg-recovered-data', 'pgbackrest-repo', 'blob-primary-data', 'blob-dr-data')
PGDATA = '/srv/kamori-postgres/postgresql/16/main'
BASE = Path('/private/tmp/kamori-readiness-20261015/restore')
RUN = BASE / datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
created = []
measurements = {}
log = None


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def command(args, *, data=None, expected=0, timeout=600):
    started = time.monotonic()
    log.write('$ ' + shlex.join(map(str, args)) + '\n')
    log.flush()
    result = subprocess.run(list(map(str, args)), input=data, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, errors='replace', env=ENV, timeout=timeout)
    log.write(result.stdout + result.stderr + f'\nexit={result.returncode} seconds={time.monotonic()-started:.3f}\n')
    log.flush()
    if expected is not None:
        require(result.returncode == expected, f'command exited {result.returncode}: {shlex.join(map(str, args))}; see {RUN}/commands.log')
    return result


def docker(*args, **kwargs):
    return command([ENGINE, *args], **kwargs)


def owned(kind, name):
    record = json.loads(docker(kind, 'inspect', name).stdout)[0]
    labels = record.get('Labels', record.get('labels')) if kind not in ('container', 'image') else record['Config']['Labels']
    require(all((labels or {}).get(k) == v for k, v in LABELS.items()), f'FOREIGN RESOURCE: {name}')


def label_args():
    return [part for k, v in LABELS.items() for part in ('--label', f'{k}={v}')]


def execute(role, *args, **kwargs):
    return docker('exec', '-i', f'{NAMESPACE}-{role}', *args, **kwargs)


def shell(role, script, **kwargs):
    return execute(role, 'bash', '-euo', 'pipefail', '-c', script, **kwargs)


def sql(role, text):
    return execute(role, 'psql', '-U', 'postgres', '-d', 'kamori', '-XAt', '-v', 'ON_ERROR_STOP=1', data=text).stdout.strip()


def copy(role, path, destination):
    docker('cp', path, f'{NAMESPACE}-{role}:{destination}')


def write(name, content):
    path = RUN / name
    path.write_text(content)
    return path


def start(role, image=IMAGE, mounts=(), env=None, args=()):
    name = f'{NAMESPACE}-{role}'
    opts = ['run', '--pull=never', '-d', '--name', name, '--network', NAMESPACE, *label_args()]
    for volume, target in mounts:
        opts += ['-v', f'{NAMESPACE}-{volume}:{target}']
    if env:
        opts += ['--env-file', str(env)]
    docker(*opts, image, *args)
    created.append(('container', name))
    owned('container', name)


def cleanup():
    errors = []
    for kind, name in reversed(created):
        try:
            owned(kind, name)
            if kind == 'container':
                result = docker('logs', name, expected=None)
                (RUN / f'{name}.log').write_text(result.stdout + result.stderr)
                docker('container', 'rm', '-f', name)
            else:
                docker(kind, 'rm', name)
        except Exception as error:
            errors.append(str(error))
    # Verify absence through successful inventories, never interpret daemon errors as absence.
    remaining = {}
    for kind in ('container', 'volume', 'network'):
        try:
            result = docker(kind, 'ls', *(['-a'] if kind == 'container' else []), '--format', '{{.Names}}' if kind == 'container' else '{{.Name}}')
            names = set(result.stdout.splitlines())
            remaining[kind] = [name for resource, name in created if resource == kind and name in names]
        except Exception as error:
            errors.append(str(error))
            remaining[kind] = ['inventory unavailable']
    write('cleanup.json', json.dumps({'errors': errors, 'remaining': remaining}, indent=2))
    require(not errors and not any(remaining.values()), 'cleanup incomplete; inspect cleanup.json')


def preflight():
    # Check all collisions before adopting the reserved network or creating anything.
    for kind, names in (('container', ROLES), ('volume', VOLUMES)):
        existing = set(docker(kind, 'ls', *(['-a'] if kind == 'container' else []), '--format', '{{.Names}}' if kind == 'container' else '{{.Name}}').stdout.splitlines())
        for suffix in names:
            require(f'{NAMESPACE}-{suffix}' not in existing, f'collision: {NAMESPACE}-{suffix}; no existing container/volume will be reused or removed')
    images = set(docker('image', 'ls', '--format', '{{.Repository}}:{{.Tag}}').stdout.splitlines())
    if IMAGE in images:
        owned('image', IMAGE)
    networks = set(docker('network', 'ls', '--format', '{{.Name}}').stdout.splitlines())
    if NAMESPACE in networks:
        owned('network', NAMESPACE)
    else:
        docker('network', 'create', *label_args(), NAMESPACE)
    created.append(('network', NAMESPACE))
    for suffix in VOLUMES:
        name = f'{NAMESPACE}-{suffix}'
        docker('volume', 'create', *label_args(), name)
        created.append(('volume', name))
        owned('volume', name)


def database():
    start('pg-source', mounts=(('pg-source-data', PGDATA), ('pgbackrest-repo', '/repository')))
    cipher = write('cipher-pass', os.urandom(32).hex())
    copy('pg-source', cipher, '/exercise/cipher-pass')
    shell('pg-source', 'bash /exercise/pg-setup.sh')
    migrator_env = write('migrator.env', f'KAMORI_DATABASE_URL=postgresql://postgres@{NAMESPACE}-pg-source:5432/kamori\n')
    start('tools', MIGRATOR, env=migrator_env, args=('migrate',))
    require(docker('wait', NAMESPACE + '-tools').stdout.strip() == '0', 'actual SQLx migration failed')
    write('sqlx-migrate.log', docker('logs', NAMESPACE + '-tools').stdout)
    owned('container', NAMESPACE + '-tools')
    docker('container', 'rm', NAMESPACE + '-tools')
    created.remove(('container', NAMESPACE + '-tools'))
    migration_manifest = []
    for path in sorted((ROOT / 'apps/cloud-server/migrations').glob('*.sql')):
        checksum = hashlib.sha384(path.read_bytes()).hexdigest()
        version = int(path.name.split('_')[0])
        actual = sql('pg-source', f"SELECT encode(checksum,'hex') FROM _sqlx_migrations WHERE version={version} AND success;")
        require(actual == checksum, f'SQLx migration {version} checksum differs from source')
        migration_manifest.append({'file': path.name, 'sha384': checksum})
    require(sql('pg-source', 'SELECT count(*) FROM _sqlx_migrations;') == str(len(migration_manifest)), 'unexpected migration count')
    write('migrations.json', json.dumps(migration_manifest, indent=2))
    sql('pg-source', (HERE / 'seed.sql').read_text())
    # Produce three deterministic binary ciphertext canaries using real AES, with test-only keys.
    objects = RUN / 'objects'
    objects.mkdir()
    manifest = []
    for index, size in enumerate((128, 4096, 65536), 1):
        name = f'ciphertext-{index}.bin'
        plaintext = bytes((index + i) % 256 for i in range(size))
        result = subprocess.run(['openssl', 'enc', '-aes-256-ctr', '-K', 'ab'*32, '-iv', f'{index:032x}'], input=plaintext, capture_output=True, check=True)
        ciphertext = result.stdout
        require(ciphertext[0] != 255, 'corruption canary must change the first byte')
        (objects / name).write_bytes(ciphertext)
        digest = hashlib.sha256(ciphertext).hexdigest()
        manifest.append(f'{digest} {size} {name}')
        sql('pg-source', f"INSERT INTO space_blobs (id,space_id,owner_user_id,ciphertext_sha256,size_padded,object_key,status) VALUES ('00000000-0000-0000-0000-{100+index:012d}','00000000-0000-0000-0000-000000000004','00000000-0000-0000-0000-000000000001',decode('{digest}','hex'),{size},'{name}','ready');")
    write('manifest.txt', '\n'.join(manifest) + '\n')
    # These values only satisfy the actual job's environment validation. Repository configuration
    # is the isolated local encrypted POSIX repository, not the production S3 repository.
    env = write('postgres.env', '\n'.join(f'{key}={value}' for key, value in {
        'POSTGRES_VERSION': '16', 'POSTGRES_PRIMARY_IP': '127.0.0.1', 'POSTGRES_DATA_DIR': PGDATA,
        'POSTGRES_APP_PASSWORD': 'synthetic', 'POSTGRES_JOBS_PASSWORD': 'synthetic',
        'PGBACKREST_S3_ENDPOINT': 'unused.invalid', 'PGBACKREST_S3_REGION': 'local',
        'PGBACKREST_S3_BUCKET': 'unused', 'PGBACKREST_S3_KEY_ID': 'synthetic',
        'PGBACKREST_S3_APPLICATION_KEY': 'synthetic', 'PGBACKREST_CIPHER_PASS': 'synthetic-unused'}.items()) + '\n')
    shell('pg-source', 'mkdir -p /etc/kamori /usr/local/lib/kamori')
    copy('pg-source', env, '/etc/kamori/postgres.env')
    for name in ('postgres-lib', 'run-pgbackrest-backup'):
        copy('pg-source', ROOT / 'deploy/postgres' / name, '/usr/local/lib/kamori/' + name)
    # Existing backup job intentionally calls the optional-env-path loader without args
    # (SC2119) and assigns the literal backup type diff (SC2209 command-name heuristic).
    shell('pg-source', 'for file in /exercise/pg-setup.sh /usr/local/lib/kamori/postgres-lib /usr/local/lib/kamori/run-pgbackrest-backup; do bash -n "$file"; done; shellcheck -e SC1091 /exercise/pg-setup.sh /usr/local/lib/kamori/postgres-lib; shellcheck -e SC1091,SC2119,SC2209 /usr/local/lib/kamori/run-pgbackrest-backup')
    started = time.monotonic()
    execute('pg-source', 'runuser', '-u', 'postgres', '--', 'pgbackrest', '--stanza=kamori', '--type=full', 'backup')
    shell('pg-source', 'bash /usr/local/lib/kamori/run-pgbackrest-backup')
    measurements['backup_and_production_job_seconds'] = time.monotonic() - started
    require(sql('pg-source', "SELECT status FROM operator_job_heartbeats WHERE job_name='postgres_backup';") == 'ok', 'backup heartbeat must be ok')
    sql('pg-source', "INSERT INTO readiness_markers(name) VALUES ('before');")
    target = sql('pg-source', 'SELECT clock_timestamp();')
    write('recovery-target.txt', target + '\n')
    query = """SELECT json_build_object(
      'users',(SELECT json_agg(row_to_json(t)) FROM (SELECT id,username,encode(encrypted_master_key,'hex') AS ciphertext,operation_bytes,blob_storage_bytes,auth_generation FROM users ORDER BY id)t),
      'spaces',(SELECT json_agg(row_to_json(t)) FROM (SELECT id,next_sequence,current_state_start_seq,operation_bytes FROM security_spaces ORDER BY id)t),
      'operations',(SELECT json_agg(row_to_json(t)) FROM (SELECT space_seq,encode(nonce,'hex') AS nonce,encode(ciphertext,'hex') AS ciphertext,encode(signature,'hex') AS signature FROM operation_log ORDER BY space_seq)t),
      'blobs',(SELECT json_agg(row_to_json(t)) FROM (SELECT object_key,size_padded,encode(ciphertext_sha256,'hex') AS sha256 FROM space_blobs ORDER BY object_key)t),
      'markers',(SELECT json_agg(row_to_json(t)) FROM (SELECT * FROM readiness_markers ORDER BY name)t),
      'migrations',(SELECT json_agg(row_to_json(t)) FROM (SELECT version,encode(checksum,'hex') AS sha384,success FROM _sqlx_migrations ORDER BY version)t));"""
    before = sql('pg-source', query)
    write('database-expected.json', before + '\n')
    require(json.loads(before)['users'][0]['operation_bytes'] == 292, 'operation accounting canary')
    require(json.loads(before)['users'][0]['blob_storage_bytes'] == 69760, 'blob accounting canary')
    schema = execute('pg-source', 'pg_dump', '-U', 'postgres', '--schema-only', 'kamori').stdout
    write('schema-source.sql', schema)
    sql('pg-source', "INSERT INTO readiness_markers(name) VALUES ('after'); UPDATE users SET auth_generation=9;")
    require(sql('pg-source', 'SELECT count(*) FROM readiness_markers;') == '2', 'both source markers committed')
    execute('pg-source', 'runuser', '-u', 'postgres', '--', 'pgbackrest', '--stanza=kamori', 'check')
    info = execute('pg-source', 'runuser', '-u', 'postgres', '--', 'pgbackrest', '--stanza=kamori', '--output=json', 'info').stdout
    write('pgbackrest-info.json', info)
    require(json.loads(info)[0]['cipher'] == 'aes-256-cbc', 'repository encryption')
    start('pg-recovered', mounts=(('pg-recovered-data', PGDATA), ('pgbackrest-repo', '/repository')))
    copy('pg-recovered', cipher, '/exercise/cipher-pass')
    shell('pg-recovered', 'bash /exercise/pg-setup.sh recovered')
    # pgBackRest 2.50 CBC may report FormatError after decrypting with the wrong key.
    # Use restore's nonzero exit and specific repository decode diagnostic, not info's
    # exit status (info can return zero while reporting an error status).
    bad = execute('pg-recovered', 'runuser', '-u', 'postgres', '--', 'env',
                  'PGBACKREST_REPO1_CIPHER_PASS=wrong-synthetic-key', 'pgbackrest',
                  '--stanza=kamori', '--type=time', f'--target={target}',
                  '--target-action=promote', 'restore', expected=None)
    diagnostic = bad.stdout + bad.stderr
    write('wrong-key.log', diagnostic + f'\nexit={bad.returncode}\n')
    require(bad.returncode != 0 and 'unable to load info file' in diagnostic
            and ('FormatError' in diagnostic or 'CryptoError' in diagnostic),
            'wrong-key restore must fail decoding actual encrypted repository metadata')
    shell('pg-recovered', f'test ! -e {PGDATA}/PG_VERSION')

    started = time.monotonic()
    execute('pg-recovered', 'runuser', '-u', 'postgres', '--', 'pgbackrest', '--stanza=kamori', '--type=time', f'--target={target}', '--target-action=promote', 'restore')
    execute('pg-recovered', 'runuser', '-u', 'postgres', '--', 'pg_ctl', '-D', PGDATA, '-l', '/tmp/postgres.log', '-w', 'start')
    after = sql('pg-recovered', query)
    write('database-restored.json', after + '\n')
    require(json.loads(before) == json.loads(after), 'restored data differs from exact pre-target manifest')
    restored_manifest = '\n'.join(f"{blob['sha256']} {blob['size_padded']} {blob['object_key']}" for blob in json.loads(after)['blobs']) + '\n'
    require(restored_manifest == (RUN / 'manifest.txt').read_text(), 'restored blob metadata differs from object manifest')
    write('manifest.txt', restored_manifest)
    require(sql('pg-recovered', "SELECT count(*) FROM readiness_markers WHERE name='after';") == '0', 'post-target marker leaked')
    require(sql('pg-recovered', 'SELECT pg_is_in_recovery();') == 'f', 'recovery must reach target and promote')
    restored_schema = execute('pg-recovered', 'pg_dump', '-U', 'postgres', '--schema-only', 'kamori').stdout
    write('schema-restored.sql', restored_schema)
    # Newer pg_dump randomizes its client-side restrict token; it is not schema.
    normalize = lambda s: '\n'.join(line for line in s.splitlines() if not line.startswith(('\\restrict ', '\\unrestrict ')))
    require(normalize(schema) == normalize(restored_schema), 'restored schema differs')
    measurements['restore_start_and_integrity_seconds'] = time.monotonic() - started
    execute('pg-recovered', 'runuser', '-u', 'postgres', '--', 'pg_ctl', '-D', PGDATA, '-m', 'fast', '-w', 'stop')
    execute('pg-recovered', 'runuser', '-u', 'postgres', '--', 'pg_checksums', '--check', '-D', PGDATA)
    for role in ('pg-source', 'pg-recovered'):
        write(role + '-postgres.log', shell(role, 'cat /tmp/postgres.log').stdout)
    return manifest


def blobs():
    root_env = write('minio.env', 'MINIO_ROOT_USER=restore-root\nMINIO_ROOT_PASSWORD=synthetic-restore-root-password\n')
    for role in ('blob-primary', 'blob-dr'):
        start(role, MINIO, ((role + '-data', '/data'),), root_env, ('server', '/data'))
    start('tools')
    settings = {
        'PRIMARY_S3_ENDPOINT': f'http://{NAMESPACE}-blob-primary:9000', 'PRIMARY_S3_REGION': 'us-east-1',
        'PRIMARY_S3_BUCKET': 'ciphertext', 'PRIMARY_S3_KEY_ID': 'restore-root', 'PRIMARY_S3_APPLICATION_KEY': 'synthetic-restore-root-password',
        'DR_S3_ENDPOINT': f'http://{NAMESPACE}-blob-dr:9000', 'DR_S3_REGION': 'us-east-1', 'DR_S3_BUCKET': 'ciphertext',
        'DR_S3_ACCESS_KEY': 'restore-replication', 'DR_S3_SECRET_KEY': 'synthetic-replication-password',
        'DR_ROOT_USER': 'restore-root', 'DR_ROOT_PASSWORD': 'synthetic-restore-root-password',
        'JOBS_DATABASE_URL': f'postgresql://postgres@{NAMESPACE}-pg-source:5432/kamori', 'BLOB_VERIFY_SAMPLE_COUNT': '3',
        # Keep production retry count; shorten only local expected-denial retry sleep.
        'RCLONE_RETRIES_SLEEP': '1s'}
    copy('tools', write('blob.env', '\n'.join(f'export {k}={shlex.quote(v)}' for k, v in settings.items()) + '\n'), '/exercise/blob.env')
    for name in ('objects', 'manifest.txt'):
        copy('tools', RUN / name, '/exercise/' + name)
    copy('tools', ROOT / 'deploy/backup/replicate-blobs', '/exercise/replicate-blobs')
    for name, actions in (('put-only', ['s3:PutObject', 's3:AbortMultipartUpload']),
                          ('replicate-verify', ['s3:PutObject', 's3:AbortMultipartUpload', 's3:ListBucket', 's3:GetObject', 's3:GetBucketLocation'])):
        policy = {'Version': '2012-10-17', 'Statement': [{'Effect': 'Allow', 'Action': actions, 'Resource': ['arn:aws:s3:::ciphertext', 'arn:aws:s3:::ciphertext/*']}]}
        copy('tools', write(name + '.json', json.dumps(policy)), '/exercise/' + name + '.json')
    # Bounded readiness probe, not a retry around a failed integrity assertion.
    for _ in range(30):
        result = execute('tools', 'python3', '-c', "import sys,urllib.request; [urllib.request.urlopen(url + '/minio/health/ready', timeout=2) for url in sys.argv[1:]]", settings['PRIMARY_S3_ENDPOINT'], settings['DR_S3_ENDPOINT'], expected=None)
        if result.returncode == 0:
            break
        time.sleep(1)
    else:
        raise RuntimeError('MinIO startup failed')
    execute('tools', 'bash', '/exercise/blob-exercise.sh', 'setup')
    denied = execute('tools', 'bash', '/exercise/blob-exercise.sh', 'replicate', expected=None)
    write('write-only-red.log', denied.stdout + denied.stderr)
    require(denied.returncode != 0 and 'Access Denied' in denied.stderr, 'write-only contract must fail with authorization error')
    require(sql('pg-source', "SELECT status FROM operator_job_heartbeats WHERE job_name='blob_replication';") == 'failed', 'denied replication heartbeat')
    execute('tools', 'bash', '/exercise/blob-exercise.sh', 'policy-green')
    started = time.monotonic()
    execute('tools', 'bash', '/exercise/blob-exercise.sh', 'replicate')
    require(sql('pg-source', "SELECT status FROM operator_job_heartbeats WHERE job_name='blob_replication';") == 'ok', 'replication heartbeat')
    measurements['replication_and_verification_seconds'] = time.monotonic() - started
    owned('container', NAMESPACE + '-blob-primary')
    docker('stop', NAMESPACE + '-blob-primary')
    require(docker('inspect', '--format', '{{.State.Running}}', NAMESPACE + '-blob-primary').stdout.strip() == 'false', 'primary must be stopped')
    started = time.monotonic()
    execute('tools', 'bash', '/exercise/blob-exercise.sh', 'read')
    measurements['dr_only_read_integrity_seconds'] = time.monotonic() - started
    for fault in ('missing', 'corrupt'):
        execute('tools', 'bash', '/exercise/blob-exercise.sh', fault)
        negative = execute('tools', 'bash', '/exercise/blob-exercise.sh', 'read', expected=None)
        require(negative.returncode != 0, f'{fault} object must fail integrity verification')
        write(fault + '-detected.log', negative.stdout + negative.stderr + f'\nexit={negative.returncode}\n')
        execute('tools', 'bash', '/exercise/blob-exercise.sh', 'repair')
        execute('tools', 'bash', '/exercise/blob-exercise.sh', 'read')
    docker('cp', f'{NAMESPACE}-tools:/exercise/restored', RUN / 'restored')


def main():
    global log
    os.umask(0o077)
    BASE.mkdir(parents=True, exist_ok=True)
    lock = BASE / '.running'
    lock.mkdir()  # Exclusive lock; do not steal another run's resources.
    RUN.mkdir()
    log = (RUN / 'commands.log').open('w')
    started = time.monotonic()
    try:
        for sig in (signal.SIGTERM, signal.SIGINT):
            signal.signal(sig, lambda number, frame: sys.exit(128 + number))
        preflight()
        docker('build', '-t', IMAGE, HERE, timeout=1800)
        write('source-sha.txt', command(['git', '-C', ROOT, 'rev-parse', 'HEAD']).stdout)
        write('images.json', docker('image', 'inspect', IMAGE, MINIO, MIGRATOR).stdout)
        database()
        blobs()
        versions = shell('tools', 'postgres --version; pgbackrest version; rclone version; mc --version; shellcheck --version; dpkg-query -W postgresql-16 pgbackrest rclone python3 openssl shellcheck').stdout
        write('versions.txt', versions)
        shell('tools', 'shellcheck -e SC1091 /exercise/blob-exercise.sh /exercise/pg-setup.sh /exercise/replicate-blobs')
        measurements['exercise_seconds'] = time.monotonic() - started
        measurements['scope'] = 'Local synthetic recovery under concurrent native/container builds; not production RPO/RTO or provider certification.'
        write('measurements.json', json.dumps(measurements, indent=2))
    finally:
        for sig in (signal.SIGTERM, signal.SIGINT):
            signal.signal(sig, signal.SIG_IGN)
        try:
            cleanup()
        finally:
            log.close()
            lock.rmdir()
    write('PASS', 'Encrypted PG16 time-target restore, accounting/schema/checksums, production jobs, credential denial, DR-only read, missing/corrupt detection and owned cleanup passed.\n')
    print(f'PASS: {RUN}')


if __name__ == '__main__':
    main()
