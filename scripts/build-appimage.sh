#!/bin/bash
# AppImage with FFmpeg (libx264, NVENC) and libx264 bundled as shared
# libraries. glibc, ALSA, Vulkan and the NVIDIA driver stay the system ones.
#
# Usage: scripts/build-appimage.sh
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# The build must happen on Debian 13: a glibc binary requires at runtime a
# glibc >= the build one, and the development distro (Fedora) has one too
# recent for the AppImage. If we are not already inside, we re-enter.
if [ -z "${VV_APPIMAGE_CONTAINER:-}" ]; then
    exec container/build-appimage.sh "$@"
fi

ARCH="$(uname -m)"
TARGET_DIR="$(realpath -m "${CARGO_TARGET_DIR:-target}")"
FFMPEG_PREFIX="$TARGET_DIR/ffmpeg-shared"
APPDIR="$TARGET_DIR/appimage/AppDir"
OUT="$TARGET_DIR/appimage/Tandem-$ARCH.AppImage"

scripts/build-ffmpeg.sh "$FFMPEG_PREFIX"

# FFMPEG_DIR points ffmpeg-sys-next at our prefix (bindings from its headers);
# PKG_CONFIG_PATH stays as a fallback.
export FFMPEG_DIR="$FFMPEG_PREFIX"
export PKG_CONFIG_PATH="$FFMPEG_PREFIX/lib/pkgconfig"

# The build machine has system ffmpeg (the tests need it): without this explicit
# -L, on vv-app's link command the /usr/lib64 path (emitted by other -sys crates,
# e.g. alsa-sys, via pkg-config) comes before ffmpeg-sys-next's and the linker
# resolves -lavcodec & co. against the system libav, producing a binary with
# sonames different from the bundled ones. The -L from RUSTFLAGS are placed by
# rustc before those of the build scripts, so our prefix wins.
# -rpath-link: GNU ld (the aarch64 default; x86_64 uses rust-lld) looks for
# the libraries needed by libavcodec/libavfilter (libx264, librubberband)
# there and not in the -L paths. It embeds nothing in the binary.
export RUSTFLAGS="-L native=$FFMPEG_PREFIX/lib -C link-arg=-Wl,-rpath-link,$FFMPEG_PREFIX/lib${RUSTFLAGS:+ $RUSTFLAGS}"

# Changing RUSTFLAGS/FFMPEG_DIR is not enough to make vv-app relink if a release
# artifact is already cached: we force it by cleaning the ffmpeg chain.
cargo clean --release -p ffmpeg-sys-next -p vv-media -p vv-app
cargo build -p vv-app --release

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib"
cp "$TARGET_DIR/release/tandem" "$APPDIR/usr/bin/"
cp "$FFMPEG_PREFIX/bin/ffmpeg" "$FFMPEG_PREFIX/bin/ffprobe" "$APPDIR/usr/bin/"
cp -a "$FFMPEG_PREFIX"/lib/*.so.* "$APPDIR/usr/lib/"
# Off the rpath: AppRun puts it on LD_LIBRARY_PATH only without a system libva.
mkdir -p "$APPDIR/usr/lib/libva"
mv "$APPDIR"/usr/lib/libva*.so* "$APPDIR/usr/lib/libva/"
cp packaging/appimage/tandem.desktop "$APPDIR/"
mkdir -p "$APPDIR/usr/share/applications"
cp packaging/appimage/tandem.desktop "$APPDIR/usr/share/applications/"
# Read by the AppImage integrators (Gear Lever, appimaged…) to register .tandem.
mkdir -p "$APPDIR/usr/share/mime/packages"
cp packaging/tandem-mime.xml "$APPDIR/usr/share/mime/packages/tandem.xml"

# The icon file names must match the Icon= key of the .desktop.
cp media/icons/svg/tandem-icon.svg "$APPDIR/tandem.svg"
for dir in media/icons/linux/hicolor/*/apps; do
    size="$(basename "$(dirname "$dir")")"
    dest="$APPDIR/usr/share/icons/hicolor/$size/apps"
    mkdir -p "$dest"
    for f in "$dir"/tandem.*; do
        cp "$f" "$dest/tandem.${f##*.}"
    done
done
cp media/icons/png/tandem-icon-256.png "$APPDIR/.DirIcon"
cp packaging/appimage/AppRun "$APPDIR/AppRun"

# The rpath must be set here, not via a cargo link-arg: the arguments after
# `--` to `cargo rustc` do not enter the fingerprint, so if the crate is
# already compiled cargo skips the link and silently ignores them, producing a
# binary without an rpath. patchelf acts on the copied file, regardless of
# the cache. --force-rpath => DT_RPATH (not RUNPATH), so it applies to
# indirect dependencies too, e.g. libavcodec -> libx264.
for bin in tandem ffmpeg ffprobe; do
    patchelf --force-rpath --set-rpath '$ORIGIN/../lib' "$APPDIR/usr/bin/$bin"
done

# Honest bundle check: the build machine has system ffmpeg, so `ldd` would
# resolve the libav from there, masking a broken bundle (missing rpath or
# uncopied library) that then blows up on a different distro.
# Instead we check that every required libav*/libx264 soname really is
# inside the AppDir.
# The codec libraries are needed by libavcodec, not by vv-app: its
# dependencies are checked too.
for so in $(patchelf --print-needed "$APPDIR"/usr/bin/* "$APPDIR"/usr/lib/libavcodec.so.*); do
    case "$so" in
        libva*)
            if [ ! -e "$APPDIR/usr/lib/libva/$so" ]; then
                echo "required library missing from the bundle: $so" >&2
                exit 1
            fi ;;
        libav*|libsw*|libpostproc*|libx264*|libmp3lame*|libopus*|libvorbis*|libogg*)
            if [ ! -e "$APPDIR/usr/lib/$so" ]; then
                echo "required library missing from the bundle: $so" >&2
                exit 1
            fi ;;
    esac
done

# The GPU drivers belong to the system: FFmpeg must load them with dlopen,
# or the AppImage would not start where they are missing. libvulkan is the
# generic loader that whisper links, libva the bundled one that loads the
# VA-API drivers; libdrm, which libva-drm links, is on every desktop with Mesa.
for so in $(patchelf --print-needed "$APPDIR"/usr/bin/* "$APPDIR"/usr/lib/*.so.*); do
    case "$so" in
        libcuda*|libnvcuvid*)
            echo "GPU driver library linked instead of loaded at run time: $so" >&2
            exit 1 ;;
    esac
done

TOOL="$TARGET_DIR/appimage/appimagetool-$ARCH.AppImage"
if [ ! -x "$TOOL" ]; then
    curl -fL -o "$TOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
    chmod +x "$TOOL"
fi
# No FUSE in containers: appimagetool extracts itself.
UPDATE_INFO="gh-releases-zsync|cpstest-tech|tandem|latest|Tandem-$ARCH.AppImage.zsync"
(
    # appimagetool writes the .zsync sidecar in its working directory.
    cd "$(dirname "$OUT")"
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" "$TOOL" -u "$UPDATE_INFO" "$APPDIR" "$OUT"
)
test -s "$OUT.zsync"

# In the container the target dir is a volume: the AppImage must be moved out.
if [ -n "${VV_APPIMAGE_OUT:-}" ]; then
    cp "$OUT" "$OUT.zsync" "$VV_APPIMAGE_OUT/"
    OUT="$VV_APPIMAGE_OUT/$(basename "$OUT")"
fi
echo "AppImage: $OUT"
