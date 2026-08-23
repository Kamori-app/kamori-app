#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
acceptance_dir="$repo_root/tests/acceptance"
runtime_dir="$acceptance_dir/.runtime"
compose_file="$acceptance_dir/compose.yaml"

select_engine() {
  if [[ -n "${KAMORI_CONTAINER_ENGINE:-}" ]]; then
    printf '%s\n' "$KAMORI_CONTAINER_ENGINE"
  elif command -v podman >/dev/null 2>&1 && podman info >/dev/null 2>&1; then
    printf '%s\n' podman
  elif command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    printf '%s\n' docker
  else
    printf '%s\n' "No working Podman or Docker engine was found." >&2
    exit 1
  fi
}

engine="$(select_engine)"
compose=("$engine" compose --project-directory "$acceptance_dir" -f "$compose_file")

write_runtime_config() {
  install -d -m 0700 "$runtime_dir"
  if [[ ! -f "$runtime_dir/cloud.env" ]]; then
    umask 077
    admin_totp_kek="$(openssl rand -base64 32 | tr -d '\n')"
    auth_totp_kek="$(openssl rand -base64 32 | tr -d '\n')"
    refresh_rotation_key="$(openssl rand -base64 32 | tr -d '\n')"
    jwt_secret="$(openssl rand -hex 48)"
    metrics_token="$(openssl rand -hex 32)"
    {
      printf 'KAMORI_ADMIN_TOTP_KEK=%s\n' "$admin_totp_kek"
      printf 'KAMORI_AUTH_TOTP_KEK=%s\n' "$auth_totp_kek"
      printf 'KAMORI_REFRESH_ROTATION_KEY=%s\n' "$refresh_rotation_key"
      printf 'KAMORI_JWT_SECRET=%s\n' "$jwt_secret"
      printf 'KAMORI_METRICS_BEARER_TOKEN=%s\n' "$metrics_token"
    } > "$runtime_dir/cloud.env"
  fi
}

build_images() {
  if [[ "${KAMORI_ACCEPTANCE_SKIP_BUILD:-false}" != "true" ]]; then
    # Keep these sequential: web and admin share the expensive Bun dependency
    # layer, while parallel classic Podman builds would install it twice.
    "${compose[@]}" build cloud
    "${compose[@]}" build web
    "${compose[@]}" build admin
  fi
}

write_opaque_setup() {
  if [[ -f "$runtime_dir/opaque-server-setup" ]]; then
    # The acceptance container runs as UID 10001, which is intentionally
    # different from the host user that generates this file. The parent
    # directory remains 0700, so making the file container-readable does not
    # expose it to other host users while keeping Docker and Podman portable.
    chmod 0644 "$runtime_dir/opaque-server-setup"
    return
  fi
  setup_tmp="$runtime_dir/opaque-server-setup.tmp"
  "${compose[@]}" --profile tools run --rm --no-deps opaque-setup > "$setup_tmp"
  test -s "$setup_tmp"
  chmod 0644 "$setup_tmp"
  mv "$setup_tmp" "$runtime_dir/opaque-server-setup"
}

wait_for_url() {
  url="$1"
  label="$2"
  for _ in $(seq 1 90); do
    if curl --fail --silent --show-error "$url" >/dev/null 2>&1; then
      return
    fi
    sleep 2
  done
  printf '%s\n' "Timed out waiting for $label at $url" >&2
  "${compose[@]}" ps >&2 || true
  "${compose[@]}" logs --no-color cloud web admin >&2 || true
  exit 1
}

up() {
  write_runtime_config
  build_images
  write_opaque_setup
  "${compose[@]}" up -d --remove-orphans
  wait_for_url "http://127.0.0.1:18080/health/ready" "cloud readiness"
  wait_for_url "http://127.0.0.1:14173/app" "web application"
  wait_for_url "http://127.0.0.1:14174/" "operator console"
  printf '%s\n' "Kamori acceptance stack is ready."
  printf '%s\n' "Web:   http://127.0.0.1:14173/app"
  printf '%s\n' "API:   http://127.0.0.1:18080"
  printf '%s\n' "Admin: http://127.0.0.1:14174"
}

down() {
  "${compose[@]}" down --remove-orphans
}

reset() {
  "${compose[@]}" down --volumes --remove-orphans
  if [[ -d "$runtime_dir" ]]; then
    find "$runtime_dir" -type f -delete
  fi
}

run_suite() {
  suite="$1"
  # Acceptance accounts and quota ledgers are intentionally disposable. Start
  # every suite from a fresh database so repeated local/CI runs cannot affect
  # one another or eventually exhaust the beta account cap.
  reset
  up
  export KAMORI_ACCEPTANCE_WEB_URL="http://127.0.0.1:14173"
  export KAMORI_ACCEPTANCE_API_URL="http://127.0.0.1:18080"
  export KAMORI_ACCEPTANCE_ADMIN_URL="http://127.0.0.1:14174"
  bun run --cwd "$acceptance_dir" "$suite"
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  reset) reset ;;
  smoke) run_suite smoke ;;
  full) run_suite full ;;
  logs) "${compose[@]}" logs --no-color "${@:2}" ;;
  *)
    printf '%s\n' "Usage: scripts/acceptance.sh {up|down|reset|smoke|full|logs [service...]}" >&2
    exit 2
    ;;
esac
