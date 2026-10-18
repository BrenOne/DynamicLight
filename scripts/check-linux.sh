#!/usr/bin/env bash
set -euo pipefail
bundle=src-tauri/target/x86_64-unknown-linux-gnu/release/bundle
cp src-tauri/linux/69-dynamiclight.rules "$bundle/69-dynamiclight.rules"
package=$(find "$bundle/deb" -maxdepth 1 -name '*.deb' -print -quit)
test -n "$package"
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
dpkg-deb -x "$package" "$scratch"
dpkg-deb -e "$package" "$scratch/control"
test -f "$scratch/usr/lib/udev/rules.d/69-dynamiclight.rules"
find "$scratch/usr/share/applications" -name '*.desktop' | grep -q .
find "$scratch/usr/share/icons" -name '*.png' | grep -q .
grep -q 'pulseaudio-utils' "$scratch/control/control"
test -x "$scratch/control/postinst"
bash -n "$scratch/control/postinst" "$scratch/control/postrm"
# No USB or screen capture starts. A startup crash must not be mistaken for
# timeout's expected exit status when stopping this intentionally idle app.
set +e
XDG_SESSION_TYPE=x11 xvfb-run -a dbus-run-session -- timeout 15s "$scratch/usr/bin/dynamiclight" > "$scratch/startup.log" 2>&1
status=$?
set -e
cat "$scratch/startup.log"
test "$status" -eq 124
! grep -Eq 'panicked at|Unable to start DynamicLight' "$scratch/startup.log"
