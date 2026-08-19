#!/bin/sh
set -eu

DEST="${WIN_OUTPUT_PATH:-/mnt/c/Users/alexa/Desktop/testbin}"
SECONDS_TO_RUN="${1:-20}"
LOG="/tmp/opends-watch-capture.log"

[ -f /tmp/opends-allowed-app.txt ] || {
    echo "capture-watch: no cleared app on record. Run hack/auto-verify.sh first." >&2
    exit 1
}

APP=$(cat /tmp/opends-allowed-app.txt)
WIN_APP=$(echo "$DEST/$APP" | sed 's|^/mnt/c|C:|; s|/|\\|g')

echo "capture-watch: recording for ${SECONDS_TO_RUN}s starting now."
echo "capture-watch: move both sticks past centre, drag a finger across the touchpad,"
echo "capture-watch: and tilt the pad in your hands, for the whole window."

timeout "$SECONDS_TO_RUN" powershell.exe -NoProfile -Command "& '$WIN_APP' --watch-pad" \
    > "$LOG" 2>&1 || true

tr -d '\r' < "$LOG" > "$LOG.clean"
mv "$LOG.clean" "$LOG"

echo ""
echo "capture-watch: done. $(wc -l < "$LOG") line(s) captured at $LOG"
echo ""
echo "--- transport seen ---"
grep -oE '(Usb|BluetoothBasic|BluetoothFull)' "$LOG" | sort -u
echo ""
echo "--- lightbar / output status ---"
grep -E "lightbar set|could not set the lightbar" "$LOG" || echo "(neither line appeared)"
echo ""
echo "--- any touch or motion seen away from rest ---"
grep -vE "touch1=up touch2=up.*gyro=0,0,0 accel=0,0,0" "$LOG" | grep -E "touch1=|gyro=" | head -5 \
    || echo "(touch and motion stayed at rest the whole capture)"
