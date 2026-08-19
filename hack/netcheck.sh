#!/bin/sh
set -eu

ROOTS="src ../opends-core/src"

BANNED='TcpStream|TcpListener|UdpSocket|std::net|SocketAddr|reqwest|hyper::|ureq|curl|WSAStartup|WinHttp|InternetOpen|socket2'

fail=0

for root in $ROOTS; do
    [ -d "$root" ] || continue

    if grep -rnE "$BANNED" "$root"; then
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    echo "" >&2
    echo "opends needs no internet. Not for updates, not for telemetry, not for" >&2
    echo "anything. A socket here is the premise of the project breaking." >&2
    echo "" >&2
    echo "See docs/security-audit.md. The reference downloads an exe and runs" >&2
    echo "it, serves gyro over UDP and talks to OpenRGB. We port none of that." >&2
    exit 1
fi

echo "no socket API in $ROOTS"
