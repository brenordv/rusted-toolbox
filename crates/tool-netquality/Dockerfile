FROM rust:1.93.0-bookworm AS builder

WORKDIR /workspace

COPY crates/shared ./crates/shared
COPY crates/tool-netquality ./crates/tool-netquality

WORKDIR /workspace/crates/tool-netquality

RUN cargo build --release

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV SPEEDTEST_CLI_PATH=/usr/local/bin/speedtest

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl tar \
    && curl -L -o /tmp/speedtest.tgz https://install.speedtest.net/app/cli/ookla-speedtest-1.2.0-linux-x86_64.tgz \
    && tar -xzf /tmp/speedtest.tgz -C /usr/local/bin speedtest \
    && rm -f /tmp/speedtest.tgz \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /workspace/crates/tool-netquality/target/release/netquality /usr/local/bin/netquality

# Adding Ookla's speed test cli to the arguments, if it was not passed.
RUN printf '%s\n' \
    '#!/bin/sh' \
    'set -e' \
    '' \
    'DEFAULT_SPEEDTEST="${SPEEDTEST_CLI_PATH:-/usr/local/bin/speedtest}"' \
    'has_speedtest_arg=0' \
    'prev=""' \
    'for arg in "$@"; do' \
    '  if [ "$prev" = "--speedtest-cli-path" ]; then' \
    '    has_speedtest_arg=1' \
    '    break' \
    '  fi' \
    '  case "$arg" in' \
    '    --speedtest-cli-path|--speedtest-cli-path=*) has_speedtest_arg=1; break ;;' \
    '  esac' \
    '  prev="$arg"' \
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