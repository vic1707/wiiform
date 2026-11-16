# Three images, one per command: build with `--target wiiform`, `wii-seed` or `wii-nand`.
# The builder and the runtime must be the same Debian release: the bundle brings
# every shared library except glibc, which the runtime supplies.
FROM ghcr.io/jdx/mise:2026.11.8-debian AS builder

RUN . /etc/os-release && test "$VERSION_ID" = 13 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git patchelf pax-utils python3 \
        qt5-qmake qtbase5-dev xz-utils \
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
RUN WIIFORM_BUILD_JOBS="$BUILD_JOBS" CARGO_BUILD_JOBS="$BUILD_JOBS" mise run build \
    && mise run assets:priiloader \
    && bash .mise/tasks/.package/linux.sh /app/bundle \
        build/bin/wiiform=bin/wiiform build/bin/wii-seed=bin/wii-seed build/bin/wii-nand=bin/wii-nand \
        build/lib/libwiiform-core.so=lib/libwiiform-core.so \
    && mkdir -p /app/bundle/share/wiiform \
    && cp -R build/share/wiiform/priiloader /app/bundle/share/wiiform/

FROM gcr.io/distroless/base-nossl-debian13:nonroot AS runtime
COPY --from=builder /app/bundle/lib /opt/wiiform/lib

FROM runtime AS wii-seed
COPY --from=builder /app/bundle/bin/wii-seed /opt/wiiform/bin/wii-seed
RUN ["/opt/wiiform/bin/wii-seed", "--version"]
ENTRYPOINT ["/opt/wiiform/bin/wii-seed"]

FROM runtime AS wii-nand
COPY --from=builder /app/bundle/share /opt/wiiform/share
COPY --from=builder /app/bundle/bin/wii-nand /opt/wiiform/bin/wii-nand
RUN ["/opt/wiiform/bin/wii-nand", "--version"]
ENTRYPOINT ["/opt/wiiform/bin/wii-nand"]

FROM runtime AS wiiform
COPY --from=builder /app/bundle/share /opt/wiiform/share
COPY --from=builder /app/bundle/bin/wiiform /opt/wiiform/bin/wiiform
RUN ["/opt/wiiform/bin/wiiform", "--version"]
ENTRYPOINT ["/opt/wiiform/bin/wiiform"]
