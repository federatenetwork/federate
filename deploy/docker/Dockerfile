# Node 1 server image
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p federate-server

FROM debian:bookworm-slim
RUN useradd -r federate
COPY --from=build /src/target/release/federate-server /usr/local/bin/
COPY sites /var/lib/federate/sites
USER federate
EXPOSE 9000
CMD ["federate-server", "--listen", "0.0.0.0:9000", "--sites-dir", "/var/lib/federate/sites", "--data-dir", "/tmp/federate-data"]
