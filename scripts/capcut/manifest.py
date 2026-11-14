# usage: python3 -I manifest.py <bake.json> <out manifest.json> <spec.json>
# spec: {"name": {...}, "category": "...", "default_secs": x, "curves": ["<python expr of v('track#i')>", ...]}
import json, re, sys, math
bake = json.load(open(sys.argv[1]))
spec = json.load(open(sys.argv[3]))
n = len(next(v for k, v in bake.items() if k != "comp"))
def box_sigma(intensity, passes):
    # CapCut LumiBoxBlur: taps at 0 and +-2k (k odd <= 0.64 * intensity), in
    # steps of short side / 1000, run `passes` times.
    n = int(intensity * 0.64)
    ks = list(range(1, n + 1, 2))
    var = sum(2 * (2 * k) ** 2 for k in ks) / (1 + 2 * len(ks))
    return math.sqrt(passes * var) / 1000

def pincushion(fov):
    f = min(max(fov, 0.0), 180.0) / 180.0
    k1 = 14.77 * f ** 10.0 + 2.7 * f ** 3.0
    k2 = 3.82 * f ** 2.54 + 0.529 * f
    return k1, k2

curves = []
for expr in spec.pop("curves"):
    vals = []
    for i in range(n):
        env = {
            "v": lambda name: bake[name][i],
            "V": lambda name, k: bake[name][min(i + k, n - 1)],
            "math": math, "i": i, "n": n,
            "G": 2 * (0.14489292176612587 * 0.0625 + 0.12264921034103102 * 0.25 + 0.09290249985875561 * 0.5625 + 0.06297020422491453),
            "B": 2 * (0.0625 + 0.25 + 0.5625 + 1.0) / 9,
            "box": box_sigma,
            "pin": pincushion,
        }
        x = eval(expr, env)
        x = round(x, 5)
        vals.append(0.0 if abs(x) < 1e-5 else x)
    curves.append(vals)
m = {"kind": "transition", **spec, "curves": curves}
text = json.dumps(m, ensure_ascii=False, indent=2)
text = re.sub(r'\[\s+([-0-9.e,\s]+?)\s+\]', lambda g: "[" + ", ".join(x.strip() for x in g.group(1).split(",")) + "]", text)
open(sys.argv[2], "w").write(text + "\n")
