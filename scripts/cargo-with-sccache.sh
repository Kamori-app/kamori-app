#!/usr/bin/env sh
set -eu

if command -v sccache >/dev/null 2>&1; then
  export RUSTC_WRAPPER=sccache
fi

exec cargo "$@"
