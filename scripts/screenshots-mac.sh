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
"$shot" docs/images/mac-dark.png 1440x900 --mac --sidebar-hidden --select --dark --scale 2
"$shot" docs/images/mac-settings.png 1440x1000 --settings --scale 2
"$shot" docs/images/mac-accounts.png 1440x1000 --accounts --scale 2
"$shot" docs/images/microsoft365-setup.png 1440x1100 --graph-setup --scale 2
"$shot" docs/images/mac-compose.png 1440x1000 --compose-preview --scale 2

"$shot" docs/images/mac-calendar.png 1440x1000 --mac --module calendar --scale 2

"$shot" docs/images/theme-omarchy.png 1440x900 --interface omarchy --select --dark --scale 2
"$shot" docs/images/theme-outlook.png 1440x900 --interface outlook --select --classic --scale 2

"$shot" docs/images/microsoft365-calendar.png 1440x1000 --mac --module calendar --connected-preview --scale 2
"$shot" docs/images/microsoft365-contacts.png 1440x1000 --mac --module people --connected-preview --scale 2

"$shot" docs/images/microsoft365-edit-event.png 1440x1000 --mac --module calendar --graph-edit event --scale 2
"$shot" docs/images/microsoft365-edit-contact.png 1440x1000 --mac --module people --graph-edit contact --scale 2
"$shot" docs/images/microsoft365-response.png 1440x1000 --mac --module calendar --graph-edit response --scale 2
"$shot" docs/images/microsoft365-pending.png 1440x1000 --mac --module people --graph-pending --scale 2
