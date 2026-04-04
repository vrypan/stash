#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 <go-binary> <rust-binary> [setup-binary]" >&2
  exit 2
fi

GO_BIN=$1
RUST_BIN=$2
SETUP_BIN=${3:-$GO_BIN}

COUNT=${COUNT:-1000}
REPEAT=${REPEAT:-20}

if [[ ! -x "$GO_BIN" ]]; then
  echo "go binary is not executable: $GO_BIN" >&2
  exit 2
fi
if [[ ! -x "$RUST_BIN" ]]; then
  echo "rust binary is not executable: $RUST_BIN" >&2
  exit 2
fi
if [[ ! -x "$SETUP_BIN" ]]; then
  echo "setup binary is not executable: $SETUP_BIN" >&2
  exit 2
fi

ROOT=$(mktemp -d "${TMPDIR:-/tmp}/stash-bench.XXXXXX")
STASH_DIR="$ROOT/stash"
trap 'rm -rf "$ROOT"' EXIT

mkdir -p "$STASH_DIR"

echo "Preparing benchmark stash in $STASH_DIR" >&2
echo "COUNT=$COUNT REPEAT=$REPEAT SETUP_BIN=$SETUP_BIN" >&2

for ((i = 1; i <= COUNT; i++)); do
  payload=$(
    printf 'entry-%d\npreview line for benchmark item %d\nmetadata line %d\n' \
      "$i" "$i" "$((i % 17))"
  )

  if (( i % 6 == 0 )); then
    printf '%s' "$payload" | \
      STASH_DIR="$STASH_DIR" "$SETUP_BIN" \
        -m "filename=file-$i.txt" \
        -m "source=bench" \
        -m "stage=raw" \
        >/dev/null
  elif (( i % 2 == 0 )); then
    printf '%s' "$payload" | \
      STASH_DIR="$STASH_DIR" "$SETUP_BIN" \
        -m "filename=file-$i.txt" \
        -m "source=bench" \
        >/dev/null
  else
    printf '%s' "$payload" | \
      STASH_DIR="$STASH_DIR" "$SETUP_BIN" \
        -m "filename=file-$i.txt" \
        >/dev/null
  fi
done

measure_once() {
  local bin=$1
  shift
  python3 - "$bin" "$STASH_DIR" "$@" <<'PY'
import os
import subprocess
import sys
import time

bin_path = sys.argv[1]
stash_dir = sys.argv[2]
args = sys.argv[3:]
env = dict(os.environ)
env["STASH_DIR"] = stash_dir

start = time.perf_counter()
result = subprocess.run(
    [bin_path, *args],
    env=env,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
    check=False,
)
elapsed = time.perf_counter() - start
if result.returncode != 0:
    raise SystemExit(result.returncode)
print(f"{elapsed:.9f}")
PY
}

measure_avg() {
  local bin=$1
  shift
  local total=0
  local i real

  measure_once "$bin" "$@" >/dev/null

  for ((i = 1; i <= REPEAT; i++)); do
    real=$(measure_once "$bin" "$@")
    total=$(awk -v a="$total" -v b="$real" 'BEGIN { printf "%.6f", a + b }')
  done

  awk -v total="$total" -v repeat="$REPEAT" 'BEGIN { printf "%.6f", total / repeat }'
}

print_row() {
  local label=$1
  local go_cmd=$2
  local rust_cmd=$3
  local go_avg rust_avg

  # shellcheck disable=SC2206
  local go_args=($go_cmd)
  # shellcheck disable=SC2206
  local rust_args=($rust_cmd)

  go_avg=$(measure_avg "$GO_BIN" "${go_args[@]}")
  rust_avg=$(measure_avg "$RUST_BIN" "${rust_args[@]}")

  printf '%-24s  go=%8.3f ms  rust=%8.3f ms\n' \
    "$label" \
    "$(awk -v s="$go_avg" 'BEGIN { print s * 1000 }')" \
    "$(awk -v s="$rust_avg" 'BEGIN { print s * 1000 }')"
}

echo
echo "Scenario results:"
print_row "ls -l -n 100" "ls -l -n 100" "ls -l -n 100 --color=false"
print_row "ls -l -m @ -p -n 100" "ls -l -m @ -p -n 100" "ls -l -m @ -p -n 100 --color=false"
print_row "log -n 100" "log -n 100 --no-color" "log -n 100 --color=false"
print_row "attr @1 --preview" "attr @1 --preview" "attr @1 --preview"
