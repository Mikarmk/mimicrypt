#!/bin/zsh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
ANDROID_CMDLINE_ROOT="$ANDROID_HOME/cmdline-tools"
ANDROID_CMDLINE_BIN="$ANDROID_CMDLINE_ROOT/latest/bin"
GEM_BIN="$HOME/.gem/ruby/2.6.0/bin"

echo "== Mimicrypt mobile doctor =="
echo "repo: $ROOT_DIR"

if command -v brew >/dev/null 2>&1; then
  echo "Homebrew: $(brew --version | head -n 1)"
else
  echo "Homebrew is missing. Install Homebrew first."
  exit 1
fi

if ! brew list --versions openjdk@17 >/dev/null 2>&1; then
  echo "Installing openjdk@17..."
  brew install openjdk@17
fi

if ! brew list --versions cocoapods >/dev/null 2>&1; then
  echo "Installing cocoapods..."
  brew install cocoapods
fi

JAVA_PREFIX="$(brew --prefix openjdk@17)"
JAVA_HOME="$JAVA_PREFIX/libexec/openjdk.jdk/Contents/Home"

if [ ! -d "$ANDROID_CMDLINE_ROOT/latest" ] && [ -x "$ANDROID_CMDLINE_ROOT/bin/sdkmanager" ]; then
  mkdir -p "$ANDROID_CMDLINE_ROOT/latest"
  mv "$ANDROID_CMDLINE_ROOT/NOTICE.txt" "$ANDROID_CMDLINE_ROOT/bin" "$ANDROID_CMDLINE_ROOT/lib" "$ANDROID_CMDLINE_ROOT/source.properties" "$ANDROID_CMDLINE_ROOT/latest/"
fi

if [ ! -x "$ANDROID_CMDLINE_BIN/sdkmanager" ]; then
  echo "Android command-line tools not found under $ANDROID_CMDLINE_BIN"
  echo "Run: cd apps/desktop-tauri && npx tauri android init"
  exit 1
fi

export JAVA_HOME
export PATH="$GEM_BIN:$(brew --prefix cocoapods)/bin:$JAVA_HOME/bin:$ANDROID_CMDLINE_BIN:$PATH"
export ANDROID_HOME
export ANDROID_SDK_ROOT="$ANDROID_HOME"

echo "JAVA_HOME=$JAVA_HOME"
echo "ANDROID_HOME=$ANDROID_HOME"
echo "pod=$(command -v pod)"
echo "sdkmanager=$(command -v sdkmanager)"

yes | sdkmanager --licenses >/dev/null || true
sdkmanager "platform-tools" "platforms;android-36" "build-tools;36.0.0" "ndk;29.0.13846066"

echo
echo "Environment is ready for:"
echo "  cd $ROOT_DIR/apps/desktop-tauri"
echo "  npm run mobile:bootstrap"
