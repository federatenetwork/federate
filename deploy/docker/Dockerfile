# Node 1 server image
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p federate-server

FROM debian:bookworm-slim
RUN useradd -r federate \
    && mkdir -p /var/lib/federate/data \
    && chown -R federate:federate /var/lib/federate
COPY --from=build /src/target/release/federate-server /usr/local/bin/
COPY sites /var/lib/federate/sites
COPY blocked_tlds.txt /var/lib/federate/blocked_tlds.txt
COPY data/blocked /var/lib/federate/blocked
USER federate
WORKDIR /var/lib/federate
EXPOSE 9000
# Keys live in the data volume; NEVER a tmpfs, or the root key (and with it
# the whole trust anchor) would change on every container restart.
VOLUME /var/lib/federate/data
CMD ["federate-server", \
     "--listen", "0.0.0.0:9000", \
     "--sites-dir", "/var/lib/federate/sites", \
     "--data-dir", "/var/lib/federate/data", \
     "--blocked-tlds", "/var/lib/federate/blocked_tlds.txt", \
     "--blocked-dir", "/var/lib/federate/blocked"]
