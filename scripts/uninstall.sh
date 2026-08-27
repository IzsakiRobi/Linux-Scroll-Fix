#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
    echo "Run with sudo: sudo ./scripts/uninstall.sh" >&2
    exit 1
fi

rm -f /usr/local/bin/linux-scroll-fixd
echo "Linux Scroll Fix removed. Configuration was kept in /etc/linux-scroll-fix."
