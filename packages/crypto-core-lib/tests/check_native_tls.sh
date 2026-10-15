#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
# Inspect each package alone and exclude dev dependencies/cloud-server feature unification.
for package in crypto-core-lib dav-bridge-desktop; do
  options=(-p "$package")
  if [[ "$package" == crypto-core-lib ]]; then
    options+=(--no-default-features --features frb)
  fi
  graph=$(rustup run "${KAMORI_RUST_TOOLCHAIN:-1.97.1}" cargo tree --locked --offline "${options[@]}" -e normal,features -i reqwest)
  if ! rg -q 'reqwest feature "rustls"' <<< "$graph"; then
    echo "Missing isolated HTTPS support: $package" >&2
    exit 1
  fi
  echo "$package: isolated reqwest rustls enabled"
done
android=$(rustup run "${KAMORI_RUST_TOOLCHAIN:-1.97.1}" cargo tree --locked --offline -p crypto-core-lib --no-default-features --features frb --target aarch64-linux-android -e normal,features)
rg -q 'webpki-roots' <<< "$android"
echo 'Android FRB: bundled Mozilla roots present'
