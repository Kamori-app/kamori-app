#!/usr/bin/env bash
# Run an iOS build/test with a scoped Xcode selection, including Dart hooks.
# Example (from apps/dav-bridge-mobile):
#   ../../scripts/with-ios-toolchain.sh flutter build ios --release --no-codesign \
#     --dart-define=KAMORI_USE_MOCK_RUST=false
set -euo pipefail

if [[ $# -eq 0 ]]; then
  echo "Usage: $0 <build-or-test-command> [arguments...]" >&2
  exit 64
fi
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
if [[ "$DEVELOPER_DIR" != /* || ! -d "$DEVELOPER_DIR/Platforms/iPhoneOS.platform" ]]; then
  echo "DEVELOPER_DIR must point to an installed Xcode Developer directory" >&2
  exit 64
fi

# hooks_runner filters DEVELOPER_DIR from native-assets hooks but preserves PATH.
# Retain that selection only for xcrun; never modify xcode-select or SDK files.
kamori_tool_bin="$(mktemp -d "${TMPDIR:-/tmp}/kamori-ios-tools.XXXXXX")"
trap 'rm -rf "$kamori_tool_bin"' EXIT
{
  printf '#!/bin/bash\n'
  printf 'export DEVELOPER_DIR=%q\n' "$DEVELOPER_DIR"
  printf 'exec /usr/bin/xcrun "$@"\n'
} > "$kamori_tool_bin/xcrun"
chmod +x "$kamori_tool_bin/xcrun"
export PATH="$kamori_tool_bin:$PATH"
"$@"
