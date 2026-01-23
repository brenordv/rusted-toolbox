FROM rust:1.78-bookworm AS builder

WORKDIR /workspace

COPY crates/shared ./crates/shared
COPY crates/tool-netquality ./crates/tool-netquality

WORKDIR /workspace/crates/tool-netquality

RUN cargo build --release

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && curl -s https://packagecloud.io/install/repositories/ookla/speedtest-cli/script.deb.sh | bash \
    && apt-get install -y --no-install-recommends speedtest \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /workspace/crates/tool-netquality/target/release/netquality /usr/local/bin/netquality

WORKDIR /data

ENTRYPOINT ["/usr/local/bin/netquality"]
CMD []
