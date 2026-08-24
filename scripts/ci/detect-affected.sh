#!/usr/bin/env bash
set -euo pipefail

# Maps changed repository paths to CI areas. Keep dependency fan-out here so
# pull requests, pushes, and local tests use one reviewable source of truth.

output_file=""
run_all=false

while (($# > 0)); do
  case "$1" in
    --output)
      if (($# < 2)); then
        echo "--output requires a path" >&2
        exit 2
      fi
      output_file=$2
      shift 2
      ;;
    --all)
      run_all=true
      shift
      ;;
    --)
      shift
      break
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 2
      ;;
  esac
done

rust=false
frontend=false
mobile=false
android=false
ios=false
server=false
infrastructure=false

mark_all() {
  rust=true
  frontend=true
  mobile=true
  android=true
  ios=true
  server=true
  infrastructure=true
}

if [[ "$run_all" == "true" ]]; then
  mark_all
else
  for file in "$@"; do
    case "$file" in
      .github/workflows/*|scripts/ci/*)
        mark_all
        ;;
    esac

    case "$file" in
      Cargo.toml|Cargo.lock|rust-toolchain.toml|apps/cloud-server/*|packages/crypto-core-lib/*|apps/dav-bridge-desktop/src-tauri/*|scripts/cargo-*.sh)
        rust=true
        ;;
    esac

    case "$file" in
      package.json|bun.lock|bunfig.toml|turbo.json|apps/*/package.json|packages/*/package.json|tests/*/package.json|apps/web-frontend/*|apps/admin-frontend/*|apps/dav-bridge-desktop/*)
        frontend=true
        ;;
    esac

    case "$file" in
      apps/dav-bridge-mobile/lib/*|apps/dav-bridge-mobile/test/*|apps/dav-bridge-mobile/pubspec.yaml|apps/dav-bridge-mobile/pubspec.lock|apps/dav-bridge-mobile/analysis_options.yaml)
        mobile=true
        android=true
        ios=true
        ;;
      apps/dav-bridge-mobile/rust_builder/*|packages/crypto-core-lib/*|flutter_rust_bridge.yaml|Cargo.toml|Cargo.lock|rust-toolchain.toml)
        android=true
        ios=true
        ;;
      apps/dav-bridge-mobile/android/*)
        android=true
        ;;
      apps/dav-bridge-mobile/ios/*)
        ios=true
        ;;
    esac

    case "$file" in
      package.json|bun.lock|bunfig.toml|apps/*/package.json|packages/*/package.json|tests/*/package.json|Cargo.toml|Cargo.lock|rust-toolchain.toml|apps/cloud-server/*|apps/web-frontend/*|apps/admin-frontend/*|packages/crypto-core-lib/*|deploy/cloud-server/*|deploy/edge/*|tests/acceptance/*|scripts/acceptance.sh)
        server=true
        ;;
    esac

    case "$file" in
      infra/*|deploy/*)
        infrastructure=true
        ;;
    esac
  done
fi

result=$(printf '%s\n' \
  "rust=$rust" \
  "frontend=$frontend" \
  "mobile=$mobile" \
  "android=$android" \
  "ios=$ios" \
  "server=$server" \
  "infrastructure=$infrastructure")

printf '%s\n' "$result"
if [[ -n "$output_file" ]]; then
  printf '%s\n' "$result" >> "$output_file"
fi
