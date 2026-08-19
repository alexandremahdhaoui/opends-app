#!/bin/sh
set -eu

FLOOR="${1:-95}"

IGNORE='(zz_generated|src/bin/|driver_install_win|payload_adapter)'

cargo llvm-cov --workspace --summary-only --ignore-filename-regex "$IGNORE" > /tmp/opends-coverage.log 2>&1

tail -25 /tmp/opends-coverage.log

LINES=$(grep '^TOTAL' /tmp/opends-coverage.log | awk '{print $10}' | tr -d '%')

echo ""
echo "coverage: ${LINES}% of lines, floor is ${FLOOR}%"

WHOLE=${LINES%%.*}

if [ "$WHOLE" -lt "$FLOOR" ]; then
    echo "" >&2
    echo "Coverage fell below the floor. Generated files, the binaries and the" >&2
    echo "Windows only adapters are already excluded, so this is hand written" >&2
    echo "code with no test." >&2
    exit 1
fi
