#!/bin/bash
# Renders an asset at the instants of the reference video's frames and tiles
# CapCut (top) over ours (bottom).
# usage: cmp.sh <asset id> <cut seconds> <duration> <A> <B> <tag> [cols]
# env: CAPCUT_WORK (default ./capcut-work), CAPCUT_VIDEO (reference mp4),
#      SHIFT (progress offset, for timing checks)
set -e
W=${CAPCUT_WORK:-./capcut-work}
V=${CAPCUT_VIDEO:?set CAPCUT_VIDEO to the reference video}
REPO=$(cd "$(dirname "$0")/../.." && pwd)
id=$1; cut=$2; d=$3; A=$4; B=$5; tag=$6; cols=${7:-8}
read f0 n p0 p1 < <(python3 -c "
import math
s=$cut-$d/2; e=$cut+$d/2
f0=math.ceil(s*30-1e-6); f1=math.floor(e*30+1e-6)
print(f0, f1-f0+1, (f0/30-s)/$d + ${SHIFT:-0}, (f1/30-s)/$d + ${SHIFT:-0})")
out=$W/out/$tag; rm -rf "$out"; mkdir -p "$out/mine" "$out/ref"
"$REPO/target/release/examples/fx_frames" "$W/assets" "$id" "$W/ref/${A}_s.rgba" "$W/ref/${B}_s.rgba" 480 360 "$out/mine" "$n" "$p0" "$p1"
ffmpeg -v error -y -i "$V" -vf "select='between(n,$f0,$((f0+n-1)))',scale=480:360" -fps_mode passthrough "$out/ref/%03d.png"
i=0; for f in "$out"/mine/*.rgba; do ffmpeg -v error -y -f rawvideo -pix_fmt rgba -s 480x360 -i "$f" "$out/mine/$(printf %03d $((i+1))).png"; i=$((i+1)); done
rows=$(( (n + cols - 1) / cols ))
ffmpeg -v error -y -i "$out/ref/%03d.png" -i "$out/mine/%03d.png" -filter_complex "[0]scale=240:180,tile=${cols}x${rows}:padding=2[a];[1]scale=240:180,tile=${cols}x${rows}:padding=2[b];[a][b]vstack=inputs=2" -frames:v 1 "$W/out/$tag.png"
echo "$W/out/$tag.png frames=$n p0=$p0 p1=$p1"
