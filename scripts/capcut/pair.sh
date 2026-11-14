#!/bin/bash
# Frames of a cmp.sh run side by side, CapCut left, ours right.
# usage: pair.sh <tag> <frame numbers...>
W=${CAPCUT_WORK:-./capcut-work}
tag=$1; shift; args=(); f=""; i=0
for n in "$@"; do p=$(printf %03d $n); args+=(-i "$W/out/$tag/ref/$p.png" -i "$W/out/$tag/mine/$p.png"); f="$f[$((2*i))][$((2*i+1))]hstack[r$i];"; i=$((i+1)); done
rows=""; for j in $(seq 0 $((i-1))); do rows="$rows[r$j]"; done
ffmpeg -v error -y "${args[@]}" -filter_complex "${f}${rows}vstack=inputs=$i" "$W/out/${tag}_pair.png" 2>/dev/null \
  || ffmpeg -v error -y "${args[@]}" -filter_complex "[0][1]hstack" "$W/out/${tag}_pair.png"
echo "$W/out/${tag}_pair.png"
