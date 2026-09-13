FROM rust:1.98.0-bookworm AS builder

WORKDIR /workspace

# Dependencies are centralized in the root manifest, so the whole workspace is
# copied and the single package is built with -p (only its graph compiles).
# The toolchain pin rides along and --locked keeps the container build on the
# committed lockfile. The pin names channel "1.98", which rustup resolves to
# the latest 1.98.x: when that drifts from the base image's toolchain, the
# build downloads the pinned one (network-dependent).
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates

RUN cargo build --release --locked -p netquality

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV SPEEDTEST_CLI_PATH=/usr/local/bin/speedtest

# The tarball hash is trust-on-first-use: Ookla publishes no signature or
# checksum to verify against, so the pin (recorded 2026-09-12) detects a
# re-published or tampered artifact rather than proving provenance. The check
# gates extraction, so a mismatch fails the build before anything is unpacked.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tar \
    && curl -fL --proto '=https' --proto-redir '=https' -o /tmp/speedtest.tgz https://install.speedtest.net/app/cli/ookla-speedtest-1.2.0-linux-x86_64.tgz \
    && echo "5690596c54ff9bed63fa3732f818a05dbc2db19ad36ed68f21ca5f64d5cfeeb7  /tmp/speedtest.tgz" | sha256sum -c - \
    && tar -xzf /tmp/speedtest.tgz -C /usr/local/bin speedtest \
    && rm -f /tmp/speedtest.tgz \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /workspace/target/release/netquality /usr/local/bin/netquality

# Adding Ookla's speed test cli to the arguments, unless it was passed or a
# config file drives the run (--config conflicts with --speedtest-cli-path;
# config-file runs pick their speedtest path via speed.speedtest_cli_path).
RUN printf '%s\n' \
    '#!/bin/sh' \
    'set -e' \
    '' \
    'DEFAULT_SPEEDTEST="${SPEEDTEST_CLI_PATH:-/usr/local/bin/speedtest}"' \
    'has_speedtest_arg=0' \
    'for arg in "$@"; do' \
    '  case "$arg" in' \
    '    --speedtest-cli-path|--speedtest-cli-path=*|--config|--config=*) has_speedtest_arg=1; break ;;' \
    '  esac' \
    'done' \
    '' \
    'if [ "$has_speedtest_arg" -eq 0 ] && [ -x "$DEFAULT_SPEEDTEST" ]; then' \
    '  set -- "$@" --speedtest-cli-path "$DEFAULT_SPEEDTEST"' \
    'fi' \
    '' \
    'exec /usr/local/bin/netquality "$@"' \
    > /usr/local/bin/entrypoint.sh \
    && chmod +x /usr/local/bin/entrypoint.sh

WORKDIR /data

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD []