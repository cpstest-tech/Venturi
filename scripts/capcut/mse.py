# mse.py <tag> <frames...>: mean squared error, CapCut vs ours, at 160x120 gray.
# Lower is closer; use it to fit constants and to catch sign/timing errors.
import os, subprocess, sys
S = os.environ.get("CAPCUT_WORK", "./capcut-work")
def load(path):
    return subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-vf", "scale=160:120", "-f", "rawvideo", "-pix_fmt", "gray", "-"], capture_output=True).stdout
tag = sys.argv[1]
tot = 0
for f in sys.argv[2:]:
    a = load(f"{S}/out/{tag}/ref/{int(f):03d}.png"); b = load(f"{S}/out/{tag}/mine/{int(f):03d}.png")
    e = sum((x - y) ** 2 for x, y in zip(a, b)) / len(a)
    tot += e
    print(f, round(e, 1), end="  ")
print("| total", round(tot, 1))
