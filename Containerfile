# One image per command: build with `--target wiiform`, `wii-seed` or `wii-nand`,
# or all three with `docker buildx bake`.
# The images hold nothing but a command and the shared libraries it loads.
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

COPY --chown=builder:builder .mise.toml mise-tasks.toml ./
RUN mise trust && mise install

ENV QT_ROOT_DIR=/usr
ARG BUILD_JOBS=2
ARG TARGETARCH
COPY --chown=builder:builder . .
# The cache mount below would otherwise create build/ owned by root.
RUN mkdir -p build/cargo
# Cargo's target directory is a cache mount: it stays out of the image layers
# and later builds reuse it. rootfs/ then receives what the runtime needs:
# every library `ldd` resolves (the loader and libc included), CA certificates
# for HTTPS downloads, and a /tmp.
RUN --mount=type=cache,id=wiiform-cargo-$TARGETARCH,target=/app/build/cargo,uid=11111,gid=11111 \
    WIIFORM_BUILD_JOBS="$BUILD_JOBS" CARGO_BUILD_JOBS="$BUILD_JOBS" mise run build \
    && mkdir -p -m 1777 rootfs/tmp \
    && ldd build/bin/* build/lib/*.so \
        | awk '$1 ~ "^/" { print $1 } $3 ~ "^/" { print $3 }' | grep -v '^/app/' | sort -u \
        | xargs cp --parents -L -t rootfs /etc/ssl/certs/ca-certificates.crt

FROM scratch AS runtime
COPY --from=builder /app/rootfs /
COPY --from=builder /app/build/lib/libwiiform-core.so /usr/local/lib/
USER 65534:65534

FROM runtime AS wii-seed
COPY --from=builder /app/build/bin/wii-seed /usr/local/bin/
RUN ["/usr/local/bin/wii-seed", "--version"]
ENTRYPOINT ["/usr/local/bin/wii-seed"]

FROM runtime AS wii-nand
COPY --from=builder /app/build/bin/wii-nand /usr/local/bin/
RUN ["/usr/local/bin/wii-nand", "--version"]
ENTRYPOINT ["/usr/local/bin/wii-nand"]

FROM runtime AS wiiform
COPY --from=builder /app/build/bin/wiiform /usr/local/bin/
RUN ["/usr/local/bin/wiiform", "--version"]
ENTRYPOINT ["/usr/local/bin/wiiform"]
