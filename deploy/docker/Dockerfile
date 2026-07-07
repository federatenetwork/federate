# Federate Network node image: Node 1 server + gateway node + DNS node + CLI.
#
# The registry (redb database), keys, and caches live in mounted volumes;
# NEVER a tmpfs, or the root key (and with it the whole trust anchor) would
# change on every container restart. Private keys are files in the data
# volume, never inside the image or the database.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release \
    -p federate-server -p federate-gatewayd -p federate-dnsd -p federate-cli

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl iptables util-linux \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/federate-server /usr/local/bin/
COPY --from=build /src/target/release/federate-gatewayd /usr/local/bin/
COPY --from=build /src/target/release/federate-dnsd /usr/local/bin/
COPY --from=build /src/target/release/federate /usr/local/bin/
# Policy data files and the official TLD seed (data, not code).
COPY blocked_tlds.txt /var/lib/federate/blocked_tlds.txt
COPY data/blocked /var/lib/federate/blocked
COPY seeds /var/lib/federate/seeds
# Demo site sources (published at runtime via `federate publish package`).
COPY sites /var/lib/federate/sites
COPY deploy/docker/entrypoint.sh /usr/local/bin/federate-entrypoint.sh
WORKDIR /var/lib/federate
EXPOSE 9600 4077
ENTRYPOINT ["/usr/local/bin/federate-entrypoint.sh"]
VOLUME /var/lib/federate/data
CMD ["federate-server", \
     "--listen", "0.0.0.0:9600", \
     "--native-listen", "0.0.0.0:4077", \
     "--data-dir", "/var/lib/federate/data", \
     "--blocked-tlds", "/var/lib/federate/blocked_tlds.txt", \
     "--blocked-dir", "/var/lib/federate/blocked"]
