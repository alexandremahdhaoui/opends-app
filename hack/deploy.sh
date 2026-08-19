#!/bin/sh
set -eu

SOURCE="../target/x86_64-pc-windows-gnu/release/OpenDS.exe"
DEST="${WIN_OUTPUT_PATH:-/mnt/c/Users/alexa/Desktop/testbin}"

[ -f "$SOURCE" ] || {
    echo "deploy: $SOURCE is missing. Build first, this script only copies." >&2
    exit 1
}

COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo nogit)

if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    COMMIT="$COMMIT-dirty"
fi

SHORT_HASH=$(md5sum "$SOURCE" | cut -c1-8)
NAME="OpenDS-$COMMIT-$SHORT_HASH.exe"

if [ -f "$DEST/$NAME" ]; then
    echo "deploy: $NAME already exists, leaving it alone"
else
    cp "$SOURCE" "$DEST/$NAME"
    echo "deploy: wrote $DEST/$NAME"
fi

echo ""
echo "Smart App Control allows per binary by hash. A build it has allowed keeps"
echo "running. Overwriting one loses that permission for good, so every build"
echo "lands under its own name and the old ones stay."
echo ""
echo "Try it:"
echo "  cd \"$DEST\" && ./$NAME --list-pads"
echo ""
echo "Recent builds:"
ls -1t "$DEST"/OpenDS-*.exe 2>/dev/null | head -8 || echo "  none yet"
