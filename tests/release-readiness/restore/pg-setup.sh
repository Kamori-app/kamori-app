#!/usr/bin/env bash
set -euo pipefail
pgdata=/srv/kamori-postgres/postgresql/16/main
install -d -o postgres -g postgres -m 0700 "$pgdata" /repository
install -d -o postgres -g postgres /etc/pgbackrest /var/log/pgbackrest /var/lib/pgbackrest /var/run/postgresql
cat > /etc/pgbackrest/pgbackrest.conf <<CONFIG
[global]
repo1-type=posix
repo1-path=/repository
repo1-cipher-type=aes-256-cbc
repo1-cipher-pass=$(cat /exercise/cipher-pass)
repo1-retention-full=2
start-fast=y
process-max=2
log-level-console=info
[kamori]
pg1-path=$pgdata
CONFIG
chown postgres:postgres /etc/pgbackrest/pgbackrest.conf
chmod 0600 /etc/pgbackrest/pgbackrest.conf
if [[ ${1:-source} == source ]]; then
  runuser -u postgres -- initdb --data-checksums --auth-local=trust --auth-host=trust -D "$pgdata"
  cat >> "$pgdata/postgresql.conf" <<'CONFIG'
listen_addresses = '*'
archive_mode = on
archive_command = 'pgbackrest --stanza=kamori archive-push %p'
track_commit_timestamp = on
CONFIG
  # Network is the dedicated local test network; there are no published ports.
  echo 'host all all all trust' >> "$pgdata/pg_hba.conf"
  runuser -u postgres -- pg_ctl -D "$pgdata" -l /tmp/postgres.log -w start
  runuser -u postgres -- createdb kamori
  runuser -u postgres -- pgbackrest --stanza=kamori stanza-create
fi
