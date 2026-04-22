#!/bin/zsh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT_DIR/apps/desktop-tauri"
ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
ANDROID_CMDLINE_ROOT="$ANDROID_HOME/cmdline-tools"
ANDROID_CMDLINE_BIN="$ANDROID_CMDLINE_ROOT/latest/bin"
GEM_BIN="$HOME/.gem/ruby/2.6.0/bin"
JAVA_HOME="${JAVA_HOME:-}"

if [ -z "$JAVA_HOME" ] && command -v brew >/dev/null 2>&1; then
  JAVA_PREFIX="$(brew --prefix openjdk@17 2>/dev/null || true)"
  if [ -n "$JAVA_PREFIX" ]; then
    JAVA_HOME="$JAVA_PREFIX/libexec/openjdk.jdk/Contents/Home"
  fi
fi

export JAVA_HOME
export PATH="$GEM_BIN:$ANDROID_CMDLINE_BIN:$PATH"
if command -v brew >/dev/null 2>&1; then
  export PATH="$(brew --prefix cocoapods 2>/dev/null || true)/bin:$PATH"
fi
export ANDROID_HOME
export ANDROID_SDK_ROOT="$ANDROID_HOME"

if [ ! -d "$ANDROID_CMDLINE_ROOT/latest" ] && [ -x "$ANDROID_CMDLINE_ROOT/bin/sdkmanager" ]; then
  mkdir -p "$ANDROID_CMDLINE_ROOT/latest"
  mv "$ANDROID_CMDLINE_ROOT/NOTICE.txt" "$ANDROID_CMDLINE_ROOT/bin" "$ANDROID_CMDLINE_ROOT/lib" "$ANDROID_CMDLINE_ROOT/source.properties" "$ANDROID_CMDLINE_ROOT/latest/"
fi

cd "$APP_DIR"

android_status=0
ios_status=0

if [ ! -d src-tauri/gen/android ]; then
  echo "== Bootstrapping Android target =="
  npx tauri android init || android_status=$?
else
  echo "Android target already present."
fi

if [ ! -d src-tauri/gen/apple ]; then
  echo "== Bootstrapping iOS target =="
  npx tauri ios init || ios_status=$?
else
  echo "iOS target already present."
fi

echo
echo "Mobile targets ready under:"
echo "  $APP_DIR/src-tauri/gen/android"
echo "  $APP_DIR/src-tauri/gen/apple"

if [ "$android_status" -ne 0 ] || [ "$ios_status" -ne 0 ]; then
  echo
  echo "Android init exit code: $android_status"
  echo "iOS init exit code: $ios_status"
  exit 1
fi
