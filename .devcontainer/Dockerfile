FROM ghcr.io/ladybirdbrowser/ladybird-ci:2026.11.12

ARG USER_UID=1000
ARG USER_GID=1000

RUN set -eu; \
    apt-get update -o APT::Update::Error-Mode=any; \
    apt-get install -y --no-install-recommends ca-certificates curl gnupg lsb-release; \
    curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key -o /tmp/llvm-snapshot.gpg.key; \
    gpg --dearmor --yes -o /usr/share/keyrings/llvm-snapshot.gpg /tmp/llvm-snapshot.gpg.key; \
    llvm_release="$(lsb_release -sc)"; \
    printf 'deb [signed-by=%s] https://apt.llvm.org/%s/ llvm-toolchain-%s-22 main\n' \
        /usr/share/keyrings/llvm-snapshot.gpg "$llvm_release" "$llvm_release" \
        > /etc/apt/sources.list.d/llvm.list; \
    apt-get update -o APT::Update::Error-Mode=any; \
    apt-get install -y --no-install-recommends \
        clang-tidy-22 libxkbcommon-dev libxkbcommon-x11-0 mesa-vulkan-drivers libvulkan1 passwd patchelf sudo \
        util-linux x11-utils x11vnc xvfb openbox novnc websockify; \
    rm -rf /var/lib/apt/lists/* /tmp/llvm-snapshot.gpg.key

RUN groupadd --non-unique --gid "${USER_GID}" vscode \
    && useradd --non-unique --uid "${USER_UID}" --gid vscode --create-home --shell /bin/bash vscode \
    && printf 'vscode ALL=(ALL) NOPASSWD:ALL\n' > /etc/sudoers.d/vscode \
    && chmod 0440 /etc/sudoers.d/vscode \
    && mkdir -p /home/vscode/.cargo /home/vscode/.rustup /home/vscode/.cache/ccache \
    && chown -R vscode:vscode /home/vscode

ENV HOME=/home/vscode \
    CARGO_HOME=/home/vscode/.cargo \
    RUSTUP_HOME=/home/vscode/.rustup \
    PATH=/home/vscode/.cargo/bin:${PATH}

# Display for `./elaris linux-ui`: a virtual screen, a window manager and a VNC server that noVNC serves to a browser.
COPY entrypoint.sh /usr/local/bin/elaris-desktop-entrypoint
COPY openbox.xml /etc/elaris-desktop/openbox.xml
RUN chmod 0755 /usr/local/bin/elaris-desktop-entrypoint

USER vscode
WORKDIR /workspaces/elaris
