#!/bin/sh
# Build a local Apple-silicon development .app; no distribution signing or notarization.
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
cargo build -p mailcraft
app="$PWD/dist/MailCraft Mac.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/debug/mailcraft "$app/Contents/MacOS/mailcraft"
cp assets/app-icon/mailcraft.icns "$app/Contents/Resources/mailcraft.icns"
sed -e 's/@APP_ID@/dev.mailcraft.mac/g' -e 's/@VERSION@/0.1.1/g' -e 's/@BUILD@/1/g' \
    -e 's/<string>MailCraft<\//<string>MailCraft Mac<\//g' \
    packaging/macos/Info.plist.in > "$app/Contents/Info.plist"
touch "$app"
printf '%s\n' "$app"
