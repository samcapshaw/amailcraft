#!/bin/sh
# Use the workspace-local toolchain if installed, otherwise use cargo from PATH.
set -eu
cd "$(dirname "$0")/.."
if [ -x "$PWD/target/cargo/bin/cargo" ]; then
    PATH="$PWD/target/cargo/bin:$PATH"
    RUSTUP_HOME="$PWD/target/rustup"
    CARGO_HOME="$PWD/target/cargo"
    export PATH RUSTUP_HOME CARGO_HOME
fi
mkdir -p target/tmp
TMPDIR="$PWD/target/tmp"
CARGO_BUILD_JOBS=4
export TMPDIR CARGO_BUILD_JOBS
exec cargo run -p mailcraft -- "$@"
