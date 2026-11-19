FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home /data snowy && mkdir -p /data && chown snowy /data
COPY --from=build /src/target/release/snowy-hub /usr/local/bin/snowy-hub
USER snowy
ENV SNOWY_HUB_ADDR=0.0.0.0:8790 SNOWY_HUB_DB=/data/snowy-hub.sqlite3
EXPOSE 8790
HEALTHCHECK --interval=30s --timeout=3s CMD ["/bin/sh", "-c", "exec 3<>/dev/tcp/127.0.0.1/8790 && printf 'GET /health HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q ok <&3"]
CMD ["snowy-hub"]
