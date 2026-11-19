#!/bin/bash
# Tandem for Windows x64: a portable .zip and an Inno Setup installer, with
# the FFmpeg DLLs (BtbN GPL shared build: libx264, rubberband, NVENC, ...),
# the Vulkan loader and the MSVC runtime bundled next to tandem.exe.
# Runs in Git Bash on Windows with the MSVC toolchain, LLVM (bindgen) and the
# Vulkan SDK (VULKAN_SDK); the installer needs Inno Setup's iscc on PATH.
#
# Usage: scripts/build-windows.sh [--fetch-ffmpeg]   (only download FFmpeg)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

case "$(uname -s)" in
    MINGW*|MSYS*) ;;
    *) echo "build-windows.sh must run in Git Bash on Windows" >&2; exit 1 ;;
esac

# Same major version as ffmpeg-sys-next; the 9.0 branch only gets fixes.
FFMPEG_ZIP="ffmpeg-n9.0-latest-win64-gpl-shared-9.0"
FFMPEG_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/$FFMPEG_ZIP.zip"

mkdir -p "${CARGO_TARGET_DIR:-target}"
TARGET_DIR="$(cd "${CARGO_TARGET_DIR:-target}" && pwd)"
FFMPEG_DIR="$TARGET_DIR/ffmpeg-windows"
OUT_DIR="$TARGET_DIR/windows"
STAGE="$OUT_DIR/Tandem"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

if [ ! -f "$FFMPEG_DIR/lib/avcodec.lib" ]; then
    rm -rf "$FFMPEG_DIR" "$TARGET_DIR/$FFMPEG_ZIP"
    curl -fsSL -o "$TARGET_DIR/ffmpeg.zip" "$FFMPEG_URL"
    unzip -q -o "$TARGET_DIR/ffmpeg.zip" -d "$TARGET_DIR"
    mv "$TARGET_DIR/$FFMPEG_ZIP" "$FFMPEG_DIR"
    rm "$TARGET_DIR/ffmpeg.zip"
fi

[ "${1:-}" = --fetch-ffmpeg ] && exit 0

export FFMPEG_DIR="$(cygpath -w "$FFMPEG_DIR")"
cargo build -p vv-app --release

rm -rf "$OUT_DIR"
mkdir -p "$STAGE"
cp "$TARGET_DIR/release/tandem.exe" "$STAGE/"
cp "$(cygpath -u "$FFMPEG_DIR")"/bin/*.dll "$STAGE/"
# Reads rotation and bitrate metadata; found next to tandem.exe. Both are
# also used by the web downloader (yt-dlp).
cp "$(cygpath -u "$FFMPEG_DIR")"/bin/ffprobe.exe "$(cygpath -u "$FFMPEG_DIR")"/bin/ffmpeg.exe "$STAGE/"
cp LICENSE "$STAGE/LICENSE.txt"

# whisper's Vulkan backend links the loader: without it next to the exe,
# a machine with no Vulkan driver could not even start the app.
VULKAN_LOADER="$(find "$(cygpath -u "$VULKAN_SDK")" -iname vulkan-1.dll -path '*64*' | head -1)"
[ -n "$VULKAN_LOADER" ] || VULKAN_LOADER=/c/Windows/System32/vulkan-1.dll
cp "$VULKAN_LOADER" "$STAGE/"

# App-local MSVC runtime: a fresh Windows may lack the VC++ redistributable.
VSWHERE="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
VS="$(cygpath -u "$("$VSWHERE" -latest -products '*' -property installationPath | tr -d '\r')")"
CRT="$(find "$VS/VC/Redist/MSVC" -type d -path '*/x64/Microsoft.VC14*.CRT' | sort | tail -1)"
[ -n "$CRT" ] || { echo "MSVC runtime not found under $VS" >&2; exit 1; }
cp "$CRT"/*.dll "$STAGE/"

# Every DLL the exe imports must be bundled or part of Windows.
OBJDUMP="$(command -v objdump || true)"
if [ -n "$OBJDUMP" ]; then
    for dll in $("$OBJDUMP" -p "$STAGE/tandem.exe" | sed -n 's/^\s*DLL Name: //p' | tr -d '\r'); do
        # API sets are resolved by the loader, not files on disk.
        case "${dll,,}" in api-ms-win-*|ext-ms-*) continue ;; esac
        if [ ! -e "$STAGE/$dll" ] && [ ! -e "/c/Windows/System32/$dll" ]; then
            echo "tandem.exe imports $dll, which is neither bundled nor part of Windows" >&2
            exit 1
        fi
    done
fi

ZIP="$OUT_DIR/Tandem-windows-x64.zip"
(cd "$OUT_DIR" && 7z a -tzip -mx=9 "$(cygpath -w "$ZIP")" Tandem >/dev/null)

if command -v iscc >/dev/null; then
    iscc -Q "-DAppVersion=$VERSION" "-DSourceDir=$(cygpath -w "$STAGE")" \
        "-DOutputDir=$(cygpath -w "$OUT_DIR")" packaging/windows/tandem.iss
fi

ls -l "$OUT_DIR"
