#!/bin/sh
# Render README screenshots from the current Mac UI and synthetic sample mail.
set -eu
cd "$(dirname "$0")/.."
if [ -x "$PWD/target/cargo/bin/cargo" ]; then
    PATH="$PWD/target/cargo/bin:$PATH"
    RUSTUP_HOME="$PWD/target/rustup"
    CARGO_HOME="$PWD/target/cargo"
    export PATH RUSTUP_HOME CARGO_HOME
fi
mkdir -p target/tmp docs/images
TMPDIR="$PWD/target/tmp"
CARGO_BUILD_JOBS=4
export TMPDIR CARGO_BUILD_JOBS
cargo build -p mailcraft-ui-egui --example ui_shot
shot="${CARGO_TARGET_DIR:-target}/debug/examples/ui_shot"
"$shot" docs/images/mac-inbox.png 1440x900 --mac --select --scale 2
"$shot" docs/images/mac-dark.png 1440x900 --mac --select --dark --scale 2
"$shot" docs/images/mac-settings.png 1440x1000 --settings --scale 2
"$shot" docs/images/mac-accounts.png 1440x1000 --accounts --scale 2
"$shot" docs/images/mac-compose.png 1440x900 --compose --scale 2
