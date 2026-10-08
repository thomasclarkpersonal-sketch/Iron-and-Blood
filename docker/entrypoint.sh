#!/bin/bash
# The dedicated server's entrypoint (M4-8): settings from the environment, and a TLS
# certificate made once, so the fingerprint players pin survives restarts (D24, M4-6).
set -euo pipefail

players="${PAX_PLAYERS:-4}"
scenario="${PAX_SCENARIO:-scenarios/two_states}"
port="${PAX_PORT:-7777}"

# A mounted certificate (/app/tls/cert.pem and key.pem) is used as it is; otherwise
# one is made on the first start and kept in the /app/tls volume.
if [ ! -f /app/tls/cert.pem ] || [ ! -f /app/tls/key.pem ]; then
    openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 3650 \
        -subj "/CN=pax-server" -keyout /app/tls/key.pem -out /app/tls/cert.pem 2>/dev/null
    echo "made a TLS certificate in /app/tls"
fi

args=(--scenario "$scenario" --bind "0.0.0.0:$port" --players "$players" --saves /app/saves
    --tls-cert /app/tls/cert.pem --tls-key /app/tls/key.pem --fingerprint-file /app/tls/fingerprint)
if [ -n "${PAX_PASSWORD_FILE:-}" ]; then args+=(--password-file "$PAX_PASSWORD_FILE"); fi
if [ -n "${PAX_ADMIN:-}" ]; then
    args+=(--admin "$PAX_ADMIN" --admin-password-file "${PAX_ADMIN_PASSWORD_FILE:?PAX_ADMIN needs PAX_ADMIN_PASSWORD_FILE}")
fi
if [ -n "${PAX_COMMANDS_PER_SECOND:-}" ]; then args+=(--commands-per-second "$PAX_COMMANDS_PER_SECOND"); fi
if [ -n "${PAX_PAUSE_AFTER:-}" ]; then args+=(--pause-after "$PAX_PAUSE_AFTER"); fi
if [ -n "${PAX_DROP_AFTER:-}" ]; then args+=(--drop-after "$PAX_DROP_AFTER"); fi
if [ -n "${PAX_UPDATES_PER_SECOND:-}" ]; then args+=(--updates-per-second "$PAX_UPDATES_PER_SECOND"); fi
if [ -n "${PAX_MAP_EVERY:-}" ]; then args+=(--map-every "$PAX_MAP_EVERY"); fi
# Anything after the image name is passed on as more pax_server arguments.
exec pax_server "${args[@]}" "$@"
