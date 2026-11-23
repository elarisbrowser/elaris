#!/bin/sh
# Runs the Linux test suite inside the Elaris container: the engine and bridge build, the bridge's CTest suite, the Rust
# checks, the release bundle, and the bundled browser run from a copy outside the build tree. Run it with
# `./elaris linux-test`, which starts the container for the chosen architecture.
#
# Results go to linux-test-results/ in the repository, one log per step plus summary.txt. The script exits non-zero if
# any step fails.
set -u
cd "$(dirname "$0")/.." || exit 1

results="linux-test-results"
rm -rf "$results"
mkdir -p "$results"
summary="$results/summary.txt"
jobs="${ELARIS_JOBS:-4}"
failed=0

record() {
    printf '%-38s %s\n' "$1" "$2" | tee -a "$summary"
}

# step NAME COMMAND...: runs the command, logs its output, and records the result.
step() {
    name="$1"
    shift
    log="$results/$(printf '%s' "$name" | tr ' /:' '___').log"
    if "$@" > "$log" 2>&1; then
        record "$name" PASS
        return 0
    fi
    record "$name" "FAIL (log: $log)"
    failed=1
    return 1
}

finish() {
    echo "" | tee -a "$summary"
    if [ "$failed" -eq 0 ]; then
        echo "All Linux checks passed on $(uname -m)." | tee -a "$summary"
    else
        echo "Some Linux checks failed on $(uname -m); see the logs above." | tee -a "$summary"
    fi
    exit "$failed"
}

# The bundled browser runs from a copy outside the build tree and must find everything through its own RUNPATHs. It
# must also take its helper processes with it when it exits.
bundled_browser() {
    relocated="$HOME/relocated"
    rm -rf "$relocated"
    mkdir -p "$relocated"
    cp -a build/linux/Elaris "$relocated/Elaris" || return 1
    env -u LD_LIBRARY_PATH LIBGL_ALWAYS_SOFTWARE=1 xvfb-run --auto-servernum \
        "$relocated/Elaris/bin/elaris" "file://$PWD/bridge/tests/smoke.html" -- \
        --snap "$PWD/$results/bundled-browser.png" || return 1
    test -s "$PWD/$results/bundled-browser.png" || return 1
    sleep 2
    if ps -eo args | grep -v grep | grep -q "$relocated/Elaris/libexec/"; then
        echo "helper processes from the bundle are still running after exit" >&2
        return 1
    fi
}

echo "Elaris Linux test suite on $(uname -m), $jobs build jobs" | tee "$summary"
echo "" | tee -a "$summary"

step "setup" ./elaris setup --skip-system-deps || finish
step "engine, bridge and test build" ./elaris build --jobs "$jobs" || finish

# The bridge tests and the Rust bridge link Ladybird's libraries from the build and the vcpkg installation.
export ELARIS_LADYBIRD_LIB_DIR="$PWD/build/cpp-ladybird/lib"
vcpkg_libs="$(ls -d "$PWD"/vendor/ladybird/Build/release/vcpkg_installed/*-linux-dynamic/lib 2>/dev/null | paste -sd: -)"
export LD_LIBRARY_PATH="$ELARIS_LADYBIRD_LIB_DIR:$vcpkg_libs"
export LIBGL_ALWAYS_SOFTWARE=1

step "bridge CTest (elaris_ladybird_*)" \
    xvfb-run --auto-servernum ctest --test-dir build/cpp-ladybird --output-on-failure --timeout 300 -R '^elaris_ladybird_'
step "Rust clippy, all features" cargo clippy --all-targets --all-features --locked -- -D warnings
step "Rust tests, all features" cargo test --locked --all-features
step "Rust FFI smoke example" xvfb-run --auto-servernum cargo run --locked --features ladybird-bridge \
    --example ladybird_smoke -- "$PWD/vendor/ladybird/Base/res" "$PWD/build/cpp-ladybird/elaris-rust-profile" \
    "$PWD/bridge/tests/smoke.html"

step "release bundle" ./elaris bundle || finish
step "bundled browser: render and exit" bundled_browser

finish
