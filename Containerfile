# One image per command: build with `--target wiiform`, `wii-seed` or `wii-nand`,
# or all three with `docker buildx bake`.
# Builder and runtime are the same Debian release, so the Qt the commands were
# linked against is the Qt apt installs in the runtime.
FROM ghcr.io/jdx/mise:2026.11.8-debian AS builder

RUN . /etc/os-release && test "$VERSION_ID" = 13 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git libncurses6 qt5-qmake qtbase5-dev \
    && rm -rf /var/lib/apt/lists/*

# xmake refuses to run as root.
WORKDIR /app
RUN groupadd -g 11111 builder \
    && useradd -l -u 11111 -g 11111 -m -s /bin/bash builder \
    && chown -R builder:builder /app /mise
USER builder

COPY --chown=builder:builder .mise.toml ./
RUN mise trust && mise install

ENV QT_ROOT_DIR=/usr
ARG BUILD_JOBS=2
COPY --chown=builder:builder . .
RUN WIIFORM_BUILD_JOBS="$BUILD_JOBS" CARGO_BUILD_JOBS="$BUILD_JOBS" mise run build

FROM debian:13-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libqt5network5t64 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/build/lib/libwiiform-core.so /usr/local/lib/
USER 65534:65534

FROM runtime AS wii-seed
COPY --from=builder /app/build/bin/wii-seed /usr/local/bin/
RUN ["wii-seed", "--version"]
ENTRYPOINT ["wii-seed"]

FROM runtime AS wii-nand
COPY --from=builder /app/build/bin/wii-nand /usr/local/bin/
RUN ["wii-nand", "--version"]
ENTRYPOINT ["wii-nand"]

FROM runtime AS wiiform
COPY --from=builder /app/build/bin/wiiform /usr/local/bin/
RUN ["wiiform", "--version"]
ENTRYPOINT ["wiiform"]
