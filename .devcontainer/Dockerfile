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
        clang-tidy-22 libxkbcommon-dev mesa-vulkan-drivers libvulkan1 passwd patchelf sudo util-linux; \
    rm -rf /var/lib/apt/lists/* /tmp/llvm-snapshot.gpg.key

RUN groupadd --non-unique --gid "${USER_GID}" vscode \
    && useradd --non-unique --uid "${USER_UID}" --gid vscode --create-home --shell /bin/bash vscode \
    && printf 'vscode ALL=(ALL) NOPASSWD:ALL\n' > /etc/sudoers.d/vscode \
    && chmod 0440 /etc/sudoers.d/vscode \
    && mkdir -p /home/vscode/.cargo /home/vscode/.rustup \
    && chown -R vscode:vscode /home/vscode

ENV HOME=/home/vscode \
    CARGO_HOME=/home/vscode/.cargo \
    RUSTUP_HOME=/home/vscode/.rustup \
    PATH=/home/vscode/.cargo/bin:${PATH}

USER vscode
WORKDIR /workspaces/elaris
