#!/usr/bin/env bash
# Offline-compile every Metal shader to catch syntax errors without a GPU.
#
# Note: the runtime tests (crates/morphogen-metal/src/runtime.rs) already compile
# the *wired* shaders via new_library_with_source and gate them against the CPU
# reference during `cargo test` on macOS. This offline gate is complementary: it
# also covers shaders not yet wired into a runtime test, and runs on GPU-less CI.
#
# Requires the Xcode 16+ Metal Toolchain component. If it is not installed this
# script SKIPS (exit 0) rather than failing, so it is safe in any environment.
# Install it once with: xcodebuild -downloadComponent MetalToolchain
set -euo pipefail

shader_dir="$(cd "$(dirname "$0")/.." && pwd)/crates/morphogen-metal/shaders"

if ! xcrun -sdk macosx -f metal >/dev/null 2>&1; then
  echo "shader-check: SKIP (no 'metal' tool on this system)"
  exit 0
fi

# Probe once: a missing toolchain component fails with a recognizable message.
probe="$(ls "$shader_dir"/*.metal | head -1)"
if err="$(xcrun -sdk macosx metal -c "$probe" -o /dev/null 2>&1)"; then
  :
elif printf '%s' "$err" | grep -q "missing Metal Toolchain"; then
  echo "shader-check: SKIP (Metal Toolchain component not installed; runtime"
  echo "             tests still validate wired shaders). To enable offline"
  echo "             checks: xcodebuild -downloadComponent MetalToolchain"
  exit 0
else
  echo "shader-check: FAIL $probe"
  printf '%s\n' "$err"
  exit 1
fi

rc=0
for f in "$shader_dir"/*.metal; do
  if xcrun -sdk macosx metal -c "$f" -o /dev/null 2>/tmp/morphogen-metalerr; then
    echo "shader-check: OK   $(basename "$f")"
  else
    echo "shader-check: FAIL $(basename "$f")"
    cat /tmp/morphogen-metalerr
    rc=1
  fi
done
exit $rc
