#!/bin/sh
# Federate container entrypoint.
#
# Same-bridge hairpin workaround: Docker excludes same-network traffic from
# published-port DNAT, so Node 1's health probes to a sibling node's PUBLIC
# IP (the only host the SSRF guard allows in a registration) would time
# out. HAIRPIN_REDIRECTS rewrites exactly those probe destinations to the
# sibling containers' static addresses, inside this container's own network
# namespace only (requires NET_ADMIN; nothing on the host changes).
#
# Format: HAIRPIN_REDIRECTS="IP:PORT=TARGET_IP:PORT,IP:PORT=TARGET_IP:PORT"
# RUN_AS="uid:gid" drops privileges after the rules are in place.
set -e

if [ -n "$HAIRPIN_REDIRECTS" ]; then
    for rule in $(echo "$HAIRPIN_REDIRECTS" | tr ',' ' '); do
        match=${rule%%=*}
        target=${rule#*=}
        ip=${match%:*}
        port=${match##*:}
        iptables -t nat -A OUTPUT -p tcp -d "$ip" --dport "$port" \
            -j DNAT --to-destination "$target"
    done
fi

if [ -n "$RUN_AS" ]; then
    exec setpriv --reuid "${RUN_AS%%:*}" --regid "${RUN_AS##*:}" --clear-groups "$@"
fi
exec "$@"
