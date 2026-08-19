#!/bin/sh
set -eu

REPO="$(cd "$(dirname "$0")/.." && pwd)"
ROOTS="$REPO/src $REPO/../opends-core/src $REPO/../opends-spec/src"

MAX="${1:-0}"

strip_tests() {
    awk '/^#\[cfg\(test\)\]/ { intest = 1 } intest && /^}/ { intest = 0; next } !intest'
}

dead=0
report=""

for root in $ROOTS; do
    [ -d "$root" ] || continue

    for file in $(find "$root" -name "*.rs"); do
        names=$(strip_tests < "$file" \
            | grep -oE "^\s*pub (async )?fn [a-z_0-9]+" \
            | sed -E 's/.*fn //' \
            | sort -u)

        for name in $names; do
            callers=0

            for other in $ROOTS; do
                [ -d "$other" ] || continue

                found=$(grep -rn --include=*.rs -- "$name" "$other" 2>/dev/null \
                    | grep -v "pub fn $name" \
                    | grep -v "pub async fn $name" \
                    | grep -vc "^$" || true)

                callers=$((callers + found))
            done

            if [ "$callers" -eq 0 ]; then
                report="$report\n  $(echo "$file" | sed "s|$REPO/../||")  $name"
                dead=$((dead + 1))
            fi
        done
    done
done

echo "architecture: $dead public function(s) with no caller anywhere"

if [ "$dead" -gt 0 ]; then
    printf "%b\n" "$report"
fi

if [ "$dead" -gt "$MAX" ]; then
    echo "" >&2
    echo "The floor is $MAX. A public function nobody calls is either not wired" >&2
    echo "into real behaviour yet, or it should not be public." >&2
    exit 1
fi
