#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
app_dir="$project_dir/dist/Mohr Frame.app"

cd "$project_dir"
cargo build --release

mkdir -p "$app_dir/Contents/MacOS"
cp "$project_dir/target/release/mohr-frame" "$app_dir/Contents/MacOS/mohr-frame"
cp "$project_dir/packaging/macos/Info.plist" "$app_dir/Contents/Info.plist"
chmod +x "$app_dir/Contents/MacOS/mohr-frame"

# Локальная ad-hoc подпись позволяет запускать собранный .app двойным щелчком.
/usr/bin/codesign --force --deep --sign - "$app_dir"

echo "Готово: $app_dir"
