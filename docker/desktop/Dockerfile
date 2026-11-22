FROM debian:trixie-slim

ARG USER_UID=1000
ARG USER_GID=1000

# Build tools and Rune's X11 libraries, Mesa's software Vulkan driver, and the virtual display with its viewer.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git pkg-config python3 \
        libxkbcommon-dev libxkbcommon-x11-0 libvulkan1 mesa-vulkan-drivers \
        xvfb x11-utils x11vnc openbox novnc websockify \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --non-unique --gid "${USER_GID}" elaris \
    && useradd --non-unique --uid "${USER_UID}" --gid elaris --create-home --shell /bin/bash elaris

COPY entrypoint.sh /usr/local/bin/elaris-desktop-entrypoint
COPY openbox.xml /etc/elaris-desktop/openbox.xml
RUN chmod 0755 /usr/local/bin/elaris-desktop-entrypoint

USER elaris
WORKDIR /workspace
ENV HOME=/home/elaris \
    PATH=/home/elaris/.cargo/bin:${PATH} \
    CARGO_TARGET_DIR=/home/elaris/target

# Create the build directory as this user, so a new named volume inherits its ownership.
RUN mkdir -p /home/elaris/target

# The repository's rust-toolchain.toml tracks stable, so the image ships the same toolchain.
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
        --component rustfmt --component clippy

ENTRYPOINT ["/usr/local/bin/elaris-desktop-entrypoint"]
CMD ["./elaris", "run", "--ui"]
