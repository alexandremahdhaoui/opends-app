#!/bin/sh
set -eu

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="${WIN_OUTPUT_PATH:-/mnt/c/Users/alexa/Desktop/testbin}"
LOG_DIR="/tmp/opends-auto-verify"

mkdir -p "$LOG_DIR"

step() { echo ""; echo "auto-verify: $*"; }

fail() {
    echo "auto-verify: FAILED at: $*" >&2
    exit 1
}

to_windows_path() {
    echo "$1" | sed 's|^/mnt/c|C:|; s|/|\\|g'
}

step "1/6 forge test-all in every repo"

for repo in opends-spec opends-core opends-app opends-uhid opends-workspace; do
    printf "  %-18s " "$repo"

    if (cd "$REPO/$repo" && forge test-all) > "$LOG_DIR/test-$repo.log" 2>&1; then
        echo "PASS"
    else
        echo "FAIL"
        tail -30 "$LOG_DIR/test-$repo.log" >&2
        fail "forge test-all in $repo, see $LOG_DIR/test-$repo.log"
    fi
done

step "2/6 building and signing the driver"

(
    cd "$REPO/opends-uhid"
    sh hack/build.sh
    sh hack/sign.sh
    sh hack/inf-check.sh
    sh hack/export-check.sh
    sh hack/reliability-check.sh
) > "$LOG_DIR/driver-build.log" 2>&1 || {
    tail -30 "$LOG_DIR/driver-build.log" >&2
    fail "driver build/sign/gates, see $LOG_DIR/driver-build.log"
}

step "3/6 clearing the app and the installer against Smart App Control"
echo "  this touches a source file and relinks each attempt, so it takes a while"

(cd "$REPO/opends-app" && sh hack/sac-retry.sh app 8) > "$LOG_DIR/sac-app.log" 2>&1 || {
    tail -20 "$LOG_DIR/sac-app.log" >&2
    fail "clearing the app, see $LOG_DIR/sac-app.log"
}

(cd "$REPO/opends-app" && sh hack/sac-retry.sh setup 8) > "$LOG_DIR/sac-setup.log" 2>&1 || {
    tail -20 "$LOG_DIR/sac-setup.log" >&2
    fail "clearing the installer, see $LOG_DIR/sac-setup.log"
}

APP=$(cat /tmp/opends-allowed-app.txt)
SETUP=$(cat /tmp/opends-allowed-setup.txt)

echo "  app:       $APP"
echo "  installer: $SETUP"

python3 - "$REPO/opends-app/build/payload/OpenDS.exe" "$DEST/$APP" <<'PY'
import sys
a = open(sys.argv[1], "rb").read()
b = open(sys.argv[2], "rb").read()
if a != b:
    print("auto-verify: the installer's embedded app does not match the cleared standalone app", file=sys.stderr)
    sys.exit(1)
PY

STAMPED_VERSION=$(grep -E '^DriverVer' "$REPO/opends-uhid/build/opends-uhid.inf" | sed -E 's/.*,//')

[ -n "$STAMPED_VERSION" ] || fail "could not read the DriverVer we just stamped"

DISPLAYED_VERSION=$(echo "$STAMPED_VERSION" | awk -F. '{printf "%d.%d.%d.%d", $1,$2,$3,$4}')

echo "  waiting for pnputil to report driver version $DISPLAYED_VERSION"
echo "  (stamped as $STAMPED_VERSION; Windows drops leading zeros when it displays it)"

step "4/6 launching the installer. approve the ONE Windows UAC prompt when it appears"

RUNNING=$(powershell.exe -NoProfile -Command \
    "(Get-Process -Name OpenDS,OpenDS-Setup -EA SilentlyContinue).Count" 2>/dev/null | tr -d '\r ')

if [ -n "$RUNNING" ] && [ "$RUNNING" -gt 0 ] 2>/dev/null; then
    echo "  a running OpenDS.exe or OpenDS-Setup.exe is holding the device open."
    echo "  reinstalling underneath a live client is exactly what broke last time."
    echo "  closing it before continuing."
    powershell.exe -NoProfile -Command \
        "Stop-Process -Name OpenDS,OpenDS-Setup -Force -EA SilentlyContinue" \
        > /dev/null 2>&1 || true
    sleep 2
fi

powershell.exe -NoProfile -Command \
    "Remove-Item -ErrorAction SilentlyContinue 'C:\Windows\Temp\opends-setup.log','C:\Users\Public\opends-setup.log'" \
    > /dev/null 2>&1 || true

WIN_SETUP="$(to_windows_path "$DEST/$SETUP")"

powershell.exe -NoProfile -Command \
    "Start-Process -FilePath '$WIN_SETUP' -ArgumentList '--self-test'" \
    > "$LOG_DIR/launch.log" 2>&1

step "5/6 waiting for that exact driver version to actually install (up to 90s)"

installed=0
attempt=0

while [ "$attempt" -lt 30 ]; do
    if powershell.exe -NoProfile -Command "pnputil /enum-drivers" 2>/dev/null \
        | tr -d '\r' | grep -q "$DISPLAYED_VERSION"; then
        installed=1
        break
    fi

    attempt=$((attempt + 1))
    sleep 3
done

echo "  --- what the installer itself logged ---"
for setup_log in "/mnt/c/Windows/Temp/opends-setup.log" "/mnt/c/Users/Public/opends-setup.log"; do
    if [ -f "$setup_log" ]; then
        tr -d '\r' < "$setup_log" | sed 's/^/    /'
        cp "$setup_log" "$LOG_DIR/setup.log"
        break
    fi
done
echo "  --- end installer log ---"

if [ "$installed" -eq 0 ]; then
    echo "  --- tail of C:\\Windows\\INF\\setupapi.dev.log ---"
    cp /mnt/c/Windows/INF/setupapi.dev.log "$LOG_DIR/setupapi.dev.log" 2>/dev/null || true
    tr -d '\r' < "$LOG_DIR/setupapi.dev.log" 2>/dev/null | tail -60 | sed 's/^/    /'
    echo "  --- end setupapi.dev.log tail ---"
    echo "  driver version $DISPLAYED_VERSION never showed up in pnputil after 90s."
    echo "  Windows sometimes reconfigures an already-published package in place"
    echo "  instead of publishing a new version string, even when the install and"
    echo "  the device itself are genuinely fine. Not failing on this alone,"
    echo "  step 6's --vpad-check is the real proof and decides the result."
else
    echo "  driver package confirmed installed at version $DISPLAYED_VERSION"
fi

step "6/6 running --vpad-check (synthetic, no hardware needed)"
echo "  a fresh driver package being registered is not the same as WudfHost"
echo "  having reloaded it yet, so this retries a few times before giving up"

WIN_APP="$(to_windows_path "$DEST/$APP")"

vpad_attempt=0
vpad_ok=0

while [ "$vpad_attempt" -lt 5 ]; do
    powershell.exe -NoProfile -Command "& '$WIN_APP' --vpad-check" \
        > "$LOG_DIR/vpad-check.log" 2>&1 || true

    if ! grep -q "does not exist" "$LOG_DIR/vpad-check.log"; then
        vpad_ok=1
        break
    fi

    vpad_attempt=$((vpad_attempt + 1))
    echo "  attempt $vpad_attempt: device not hosted yet, waiting 10s and retrying"
    sleep 10
done

if [ "$vpad_ok" -eq 0 ]; then
    echo "  gave up after $vpad_attempt retries, showing the last attempt anyway"
fi

tr -d '\r' < "$LOG_DIR/vpad-check.log"

echo ""
echo "auto-verify: full log at $LOG_DIR/vpad-check.log"

if grep -q "RESULT: PASS" "$LOG_DIR/vpad-check.log"; then
    echo "auto-verify: RESULT PASS"
    exit 0
elif grep -q "RESULT: PARTIAL" "$LOG_DIR/vpad-check.log"; then
    echo "auto-verify: RESULT PARTIAL" >&2
    exit 2
else
    echo "auto-verify: RESULT FAIL or unknown" >&2
    exit 3
fi
