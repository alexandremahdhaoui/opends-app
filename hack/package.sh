#!/bin/sh
set -eu

TARGET=x86_64-pc-windows-gnu
RELEASE="../target/$TARGET/release"
STAGE="${OPENDS_PAYLOAD_DIR:-$PWD/build/payload}"
DEST="${WIN_OUTPUT_PATH:-/mnt/c/Users/alexa/Desktop/testbin}"
DRIVER="../opends-uhid/build"

die() { echo "package: $*" >&2; exit 1; }

echo "package: 1 of 4, building OpenDS.exe"

cargo build --release --target "$TARGET" -p opends-app --bin OpenDS

echo "package: 2 of 4, staging the payload"

mkdir -p "$STAGE"
cp "$RELEASE/OpenDS.exe" "$STAGE/"

for leaf in opends-uhid.dll opends-uhid.inf opends-uhid.cat opends.cer; do
    if [ -f "$DRIVER/$leaf" ]; then
        cp "$DRIVER/$leaf" "$STAGE/"
    elif [ -f "$DEST/opends-uhid/$leaf" ]; then
        cp "$DEST/opends-uhid/$leaf" "$STAGE/"
    else
        die "$leaf is missing. Build opends-uhid and sign it first."
    fi
done

echo "package: 3 of 4, building the installer with the payload inside"

OPENDS_PAYLOAD_DIR="$STAGE" cargo build --release --target "$TARGET" \
    -p opends-app --bin OpenDS-Setup

echo "package: 4 of 4, deploying"

COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo nogit)

if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    COMMIT="$COMMIT-dirty"
fi

HASH=$(md5sum "$RELEASE/OpenDS-Setup.exe" | cut -c1-8)
NAME="OpenDS-Setup-$COMMIT-$HASH.exe"

cp "$RELEASE/OpenDS-Setup.exe" "$DEST/$NAME"

SIZE=$(du -h "$DEST/$NAME" | cut -f1)

echo ""
echo "package: $DEST/$NAME  ($SIZE)"
echo ""
echo "That one file carries the driver, the certificate and OpenDS.exe."
echo "Nothing else needs to sit beside it."
echo ""
echo "Smart App Control allows per binary by hash, so this build has its own"
echo "name and earlier ones are left alone."
