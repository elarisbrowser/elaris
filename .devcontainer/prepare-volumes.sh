#!/bin/sh
set -eu

workspace=$(pwd)
uid=${ELARIS_VOLUME_UID:-$(id -u)}
gid=${ELARIS_VOLUME_GID:-$(id -g)}
owner="${uid}:${gid}"
vcpkg_build_dir=${ELARIS_VCPKG_BUILD_DIR:-"${workspace}/.git/modules/Build"}

for directory in \
    /home/vscode/.cargo \
    /home/vscode/.rustup \
    "${workspace}/build" \
    "${workspace}/target" \
    "${workspace}/vendor/ladybird/Build" \
    "${vcpkg_build_dir}"
do
    current_owner=$(stat -c '%u:%g' "${directory}")
    if [ "${current_owner}" != "${owner}" ]; then
        printf 'Fixing Docker volume ownership: %s\n' "${directory}"
        if [ "$(id -u)" -eq 0 ]; then
            chown -R "${owner}" "${directory}"
        else
            sudo chown -R "${owner}" "${directory}"
        fi
    fi
done
