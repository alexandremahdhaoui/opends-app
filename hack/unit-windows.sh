#!/bin/sh
set -eu

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
TARGET_DIR="$REPO/target/x86_64-pc-windows-gnu/debug/deps"

if [ -d "$TARGET_DIR" ]; then
    find "$TARGET_DIR" -maxdepth 1 -name 'opends_app-*.exe' -delete
fi

cd "$REPO/opends-app"

exec cargo test -p opends-app --target x86_64-pc-windows-gnu
