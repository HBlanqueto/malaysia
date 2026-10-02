#!/usr/bin/env bash
# Validate a Yuki debug capture against an expected colour.
#
# Usage:
#   capture-validate.sh <compositor-pid> [expected-rgb]
#   capture-validate.sh <capture.png> <expected-rgb>
#
# Sends SIGUSR1 (which asks the next frame to write a debug PNG), waits, then
# decodes the newest capture with ffmpeg and checks that a sample of pixels
# match the expected colour within ±1 per channel. Default expected colour is
# the Solid Lavender wallpaper (215, 207, 239).
#
# Exits 0 (PASS) or 1 (FAIL).

set -euo pipefail
EXPECTED="${2:-215 207 239}"
RUN_DIR="${XDG_RUNTIME_DIR:-/tmp}/yuki"

if [ -f "$1" ]; then
  CAP="$1"
else
  PID="$1"
  kill -USR1 "$PID"
  sleep 1.5
  CAP=$(ls -t "$RUN_DIR"/capture-*.png 2>/dev/null | head -1)
  if [ -z "$CAP" ]; then
    echo "FAIL: no capture written under $RUN_DIR" >&2
    exit 1
  fi
  echo "capture: $CAP"
fi

RAW=$(ffmpeg -v error -i "$CAP" -f rawvideo -pix_fmt rgb24 - 2>/dev/null)
read -r ew eg eb <<<"$EXPECTED"
W=1280
ok=1
for spec in "640 400" "40 600" "1240 700" "300 500"; do
  read -r X Y <<<"$spec"
  O=$(( (Y * W + X) * 3 ))
  R=${RAW:$O:3}
  R=$((16#${R:0:2})); G=$((16#${R:2:2})); B=$((16#${R:4:2}))
  # ffmpeg raw rgb24: bytes are binary; parse via od
  PX=$(printf '%s' "$RAW" | od -An -tu1 -j $O -N 3)
  read -r R G B <<<"$PX"
  if [ $((R > ew + 1 || R < ew - 1 || G > eg + 1 || G < eg - 1 || B > eb + 1 || B < eb - 1)) -eq 1 ]; then
    echo "MISMATCH @($X,$Y): ($R,$G,$B) esperado ($ew,$eg,$eb)"
    ok=0
  fi
done
if [ $ok -eq 1 ]; then
  echo "VALIDACIÓN: PASA (captura $CAP es $ew,$eg,$eb ±1)"
  exit 0
else
  echo "VALIDACIÓN: FALLA"
  exit 1
fi