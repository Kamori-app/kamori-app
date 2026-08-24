#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
detector="$script_dir/detect-affected.sh"

assert_case() {
  local label=$1
  local expected=$2
  shift 2

  local actual
  actual=$(bash "$detector" -- "$@")
  if [[ "$actual" != "$expected" ]]; then
    echo "case failed: $label" >&2
    diff -u <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") >&2 || true
    exit 1
  fi
}

none=$(printf '%s\n' \
  'rust=false' \
  'frontend=false' \
  'mobile=false' \
  'android=false' \
  'ios=false' \
  'server=false' \
  'infrastructure=false')

web=$(printf '%s\n' \
  'rust=false' \
  'frontend=true' \
  'mobile=false' \
  'android=false' \
  'ios=false' \
  'server=true' \
  'infrastructure=false')

mobile_shared=$(printf '%s\n' \
  'rust=false' \
  'frontend=false' \
  'mobile=true' \
  'android=true' \
  'ios=true' \
  'server=false' \
  'infrastructure=false')

ios_only=$(printf '%s\n' \
  'rust=false' \
  'frontend=false' \
  'mobile=false' \
  'android=false' \
  'ios=true' \
  'server=false' \
  'infrastructure=false')

cloud=$(printf '%s\n' \
  'rust=true' \
  'frontend=false' \
  'mobile=false' \
  'android=false' \
  'ios=false' \
  'server=true' \
  'infrastructure=false')

host_config=$(printf '%s\n' \
  'rust=false' \
  'frontend=false' \
  'mobile=false' \
  'android=false' \
  'ios=false' \
  'server=false' \
  'infrastructure=true')

all=$(printf '%s\n' \
  'rust=true' \
  'frontend=true' \
  'mobile=true' \
  'android=true' \
  'ios=true' \
  'server=true' \
  'infrastructure=true')

assert_case "documentation only" "$none" docs/README.md
assert_case "web application" "$web" apps/web-frontend/src/routes/+page.svelte
assert_case "shared mobile Dart" "$mobile_shared" apps/dav-bridge-mobile/lib/main.dart
assert_case "iOS platform" "$ios_only" apps/dav-bridge-mobile/ios/Runner/Info.plist
assert_case "cloud server" "$cloud" apps/cloud-server/src/main.rs
assert_case "host configuration" "$host_config" deploy/host-config/kamori-apply-host-config
assert_case "CI definition" "$all" .github/workflows/ci.yml

actual_all=$(bash "$detector" --all)
if [[ "$actual_all" != "$all" ]]; then
  echo "case failed: explicit full run" >&2
  diff -u <(printf '%s\n' "$all") <(printf '%s\n' "$actual_all") >&2 || true
  exit 1
fi
