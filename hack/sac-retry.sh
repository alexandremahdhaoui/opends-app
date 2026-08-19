#!/bin/sh
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${WIN_OUTPUT_PATH:-/mnt/c/Users/alexa/Desktop/testbin}"
WHICH="${1:-app}"
ATTEMPTS="${2:-6}"

cd "$REPO"

case "$WHICH" in
app)
    PROBE_ARGS="--list-pads"
    EXPECT="pad\(s\) found|no Sony pad found"
    TOUCH="src/bin/opends.rs"
    ;;
setup)
    PROBE_ARGS="--probe"
    EXPECT="Smart App Control allowed this build"
    TOUCH="src/bin/opends-setup.rs"
    ;;
*)
    echo "usage: sac-retry.sh [app|setup] [attempts]" >&2
    exit 2
    ;;
esac

latest_candidate() {
    if [ "$WHICH" = "app" ]; then
        ls -1t "$DEST"/OpenDS-*.exe 2>/dev/null | grep -v -- '-Setup-' | head -1
    else
        ls -1t "$DEST"/OpenDS-Setup-*.exe 2>/dev/null | head -1
    fi
}

allowed() {
    name="$1"

    [ -f "$DEST/$name" ] || {
        echo "sac-retry: $DEST/$name does not exist" >&2
        return 1
    }

    ( cd "$DEST" && timeout 40 ./"$name" $PROBE_ARGS 2>&1 ) > /tmp/sac-probe.log 2>&1

    if grep -q "Invalid argument" /tmp/sac-probe.log; then
        return 1
    fi

    grep -qE "$EXPECT" /tmp/sac-probe.log
}

build() {
    touch "$TOUCH"

    if [ "$WHICH" = "app" ]; then
        cargo build --release --target x86_64-pc-windows-gnu -p opends-app --bin OpenDS \
            > /tmp/sac-build.log 2>&1 || return 1
        sh hack/deploy.sh > /tmp/sac-deploy.log 2>&1
    else
        sh hack/package.sh > /tmp/sac-build.log 2>&1 || return 1
    fi
}

START_EPOCH=$(date +%s)
attempt=1

while [ "$attempt" -le "$ATTEMPTS" ]; do
    cd "$REPO"

    build || {
        echo "sac-retry: build failed" >&2
        tail -15 /tmp/sac-build.log >&2
        exit 1
    }

    candidate="$(latest_candidate)"

    [ -n "$candidate" ] || {
        echo "sac-retry: no OpenDS binary found in $DEST after a build" >&2
        exit 1
    }

    name=$(basename "$candidate")
    file_epoch=$(stat -c %Y "$DEST/$name" 2>/dev/null || echo 0)

    if [ "$file_epoch" -lt "$((START_EPOCH - 5))" ]; then
        echo "sac-retry: $name predates this run and was not just built." >&2
        echo "sac-retry: the file-matching pattern found a stale binary instead" >&2
        echo "sac-retry: of what build() just produced. Fix the pattern, do not retry." >&2
        exit 1
    fi

    printf "sac-retry: attempt %s, %s ... " "$attempt" "$name"

    if allowed "$name"; then
        echo "ALLOWED"
        echo ""
        echo "Smart App Control let this build run:"
        echo "  $name"
        echo "$name" > "/tmp/opends-allowed-$WHICH.txt"
        exit 0
    fi

    echo "blocked"

    attempt=$((attempt + 1))
done

echo "" >&2
echo "sac-retry: $ATTEMPTS builds of '$WHICH' were all refused. Roughly one in" >&2
echo "three clears, so run it again. Retrying the same bytes never helps, which" >&2
echo "is why every attempt relinks to move the hash." >&2
exit 1
