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
assert_case "frontend server launcher" "$web" scripts/frontend-server.mjs
assert_case "frontend server contracts" "$web" scripts/frontend-server.test.mjs
assert_case "web application" "$web" apps/web-frontend/src/routes/+page.svelte

# These external inputs drive the desktop/admin default browser gates. A PR
# changing only one must select frontend CI, rather than only acceptance smoke.
for ui_input in \
  tests/acceptance/ui-gates.config.ts \
  tests/acceptance/specs/uiHost.ts \
  tests/acceptance/specs/desktop-ui.spec.ts \
  tests/acceptance/specs/admin-ui.spec.ts \
  tests/acceptance/specs/invite-ui.spec.ts \
  tests/acceptance/fixtures/uiGates.bundle.mjs \
  tests/acceptance/fixtures/desktopUi.js \
  tests/acceptance/fixtures/adminUi.js \
  tests/acceptance/fixtures/inviteUi.js \
  tests/acceptance/specs/pim-recurrence.spec.ts \
  tests/acceptance/fixtures/calendarCorpus.js \
  tests/acceptance/fixtures/taskEditor.bundle.mjs; do
  assert_case "frontend UI input: $ui_input" "$web" "$ui_input"
done

acceptance_only=$(printf '%s\n' \
  'rust=false' \
  'frontend=false' \
  'mobile=false' \
  'android=false' \
  'ios=false' \
  'server=true' \
  'infrastructure=false')
assert_case "unrelated acceptance spec" "$acceptance_only" tests/acceptance/specs/smoke.spec.ts
assert_case "unrelated acceptance fixture" "$acceptance_only" tests/acceptance/fixtures/browserDurability.js
assert_case "shared mobile Dart" "$mobile_shared" apps/dav-bridge-mobile/lib/main.dart
assert_case "iOS platform" "$ios_only" apps/dav-bridge-mobile/ios/Runner/Info.plist
assert_case "cloud server" "$cloud" apps/cloud-server/src/main.rs
assert_case "host configuration" "$host_config" deploy/host-config/kamori-install-host-config
assert_case "CI definition" "$all" .github/workflows/ci.yml

actual_all=$(bash "$detector" --all)
if [[ "$actual_all" != "$all" ]]; then
  echo "case failed: explicit full run" >&2
  diff -u <(printf '%s\n' "$all") <(printf '%s\n' "$actual_all") >&2 || true
  exit 1
fi

# Shared authored corpus is consumed by Rust, web and Flutter unit tests only.
corpus=$(printf '%s\n' \
  'rust=true' \
  'frontend=true' \
  'mobile=true' \
  'android=false' \
  'ios=false' \
  'server=false' \
  'infrastructure=false')
for corpus_input in tests/fixtures/pim/zoned-exceptions.ics tests/fixtures/pim/custom-contact.vcf tests/fixtures/pim/corpus.json; do
  assert_case "shared PIM corpus: $corpus_input" "$corpus" "$corpus_input"
done
