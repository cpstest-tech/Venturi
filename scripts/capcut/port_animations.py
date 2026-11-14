#!/usr/bin/env python3
"""Port the fourteen reference-project animations using CapCut's Lua evaluators.

Usage: python3 scripts/capcut/port_animations.py draft_content.json Cache/effect
The original cache is read only. No CapCut engine or Python packages are needed.
"""
import argparse
import json
import math
import pathlib
import subprocess
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('draft', type=pathlib.Path)
parser.add_argument('cache', type=pathlib.Path)
parser.add_argument('--output', type=pathlib.Path, default=pathlib.Path('crates/vv-render/assets/animations'))
args = parser.parse_args()
scripts = pathlib.Path(__file__).resolve().parent
root = args.output
shader = (scripts / 'animation.wgsl').read_text()
draft = json.loads(args.draft.read_text())
by_id = {a['id']: a for material in draft['materials']['material_animations'] for a in material['animations']}
order = ['6740867832570974733', '7548872560352513281', '7241878375516606978', '6798320902548230669', '7548882779841285377', '7211044782489997825', '6740122563692728844', '7315336764636271105', '7622940867749367056', '7211044701367964162', '7266026841360568833', '6795425591014199822', '6759078592740594184', '7032223355365233154']
unsupported = set(by_id) - set(order)
if unsupported:
    raise SystemExit(f'Unsupported effect IDs: {sorted(unsupported)}')
inventory = [by_id[k] for k in order]
baked = {}
for animation in inventory:
    resource = args.cache / animation['id'] / pathlib.PureWindowsPath(animation['path']).name
    duration = animation['duration'] / 1000000.0
    if (resource / 'Transform.lua').is_file():
        mode, file = ('transform', resource / 'Transform.lua')
    elif (resource / 'AmazingFeature/lua/LumiFamily').is_dir():
        result = subprocess.check_output(['lua', str(scripts / 'bake.lua'), str(resource / 'AmazingFeature/lua/LumiFamily'), '401'])
        baked[animation['id']] = json.loads(result)
        continue
    elif (resource / 'AmazingFeature/lua/src/AE.lua').is_file():
        mode, file = ('ae', resource / 'AmazingFeature/lua/src/AE.lua')
        duration = 0.6 if animation['id'] == '7241878375516606978' else 17 / 30
    else:
        mode = 'distort' if animation['id'] == '7032223355365233154' else 'seek'
        file = resource / 'AmazingFeature/lua/SeekModeScript.lua'
        duration = 0.5 if animation['id'] == '7315336764636271105' else 1.0
    result = subprocess.check_output(['lua', str(scripts / 'bake_animation.lua'), mode, str(file), '401', str(duration)])
    baked[animation['id']] = json.loads(result)
names = [('zoom_2', 'Zoom 2'), ('stretch_out', 'Stretch out'), ('side_slide', 'Side slide'), ('fade_out', 'Fade out'), ('stretch_in', 'Stretch in'), ('flash_out', 'Flash out'), ('swing_top_left', 'Swing top left'), ('slide_up', 'Slide up'), ('blur_slide_in', 'Blur slide in'), ('flash_in', 'Flash in'), ('motion_slide', 'Motion slide'), ('bounce_1', 'Bounce 1'), ('zoom_1', 'Zoom 1'), ('distort_stretch', 'Distort and stretch')]
for a, (slug, en) in zip(inventory, names):
    b = baked[a['id']]
    N = 401
    cols = [[0.0] * N for _ in range(22)]
    for j in [2, 3, 5, 18, 19]:
        cols[j] = [1.0] * N

    def v(k, i, default=0):
        return b.get(k, [default] * N)[i]
    for i in range(N):
        p = i / (N - 1)
        c = [0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        if 'position#0' in b:
            c[0] = v('position#0', i) / 2
            c[1] = v('position#1', i) / 2
            c[2] = v('scale#0', i, 1)
            c[3] = v('scale#1', i, 1)
            c[4] = math.radians(v('rotation#2', i))
            c[5] = v('alpha', i, 1)
            blur = v('blur', i)
            if v('blur_type', i) == 2:
                c[6] = blur
            elif v('blur_type', i) == 1:
                c[7] = abs(blur) * 25 / 1080 / math.sqrt(2)
                c[8] = c[7] * 1080 / 1920
        elif slug in ['stretch_in', 'stretch_out']:
            j = N - 1 - i if slug == 'stretch_out' else i

            def lumi(k, default=0):
                return v('LumiMotionBlur3D_58-effect0#' + k, j, default)
            c[0] = lumi('position#vector#0', 0.5) - 0.5
            c[2] = c[3] = lumi('scale_x#number#0', 1)
            c[4] = -math.radians(lumi('rotation_z#number#0'))
            c[13] = math.radians(lumi('rotation_y#number#0'))
            c[5] = v('LumiLayer_58-trs#opacity#number#0', j, 100) / 100
            c[10] = v('LumiScaleWipe_41-effect0#stretch#number#0', j)
            c[15] = v('LumiDeepGlow_41-effect1#radius#number#0', j) / 1000
        elif slug == 'blur_slide_in':
            c[0] = v('LumiLayer_28-trs#position#vector#0', i, 540) / 1080 - 0.5
            c[7] = v('LumiDirectionalBlurs_28-effect0#blurIntensity#number#0', i) / 1000 / 3
        elif slug == 'side_slide':
            c[0] = v('ADBE Position_23#0', i, 540) / 1080 - 0.5
            c[1] = v('ADBE Position_23#1', i, 540) / 1080 - 0.5
            c[2] = v('ADBE Scale_27#0', i, 100) / 100
            c[3] = v('ADBE Scale_27#1', i, 100) / 100
            c[12] = math.radians(v('ADBE Rotate X_29#0', i))
            c[13] = math.radians(-v('ADBE Rotate Y_30#0', i))
            c[14] = v('ADBE Position_23#2', i) / 1500
            c[9] = v('ADBE Exposure2-0003#0', i) / 2.2
            c[7] = v('ADBE Gaussian Blur 2-0001_12#0', i) * 0.5 * 0.6 * 1.5 / 1080
            c[8] = v('ADBE Gaussian Blur 2-0001_17#0', i) * 0.5 * 0.6 * 1.5 / 1920
        elif slug == 'motion_slide':
            c[0] = v('ADBE Position_0_38#0', i, 540) / 1080 - 0.5
            c[1] = v('ADBE Position_1_39#0', i, 1080) / 1080 - 1
            c[4] = -math.radians(v('ADBE Rotate Z_45#0', i))
            c[5] = v('ADBE Opacity_46#0', i, 100) / 100
            c[9] = v('ADBE Exposure2-0003_14#0', i) / 2.2
        elif slug == 'slide_up':
            c[1] = (v('ADBE_Position_0_0#1', i, 960) - 960) / 1920
            c[7] = v('ADBE_IE_f27e10b94d011bf31c57ec1-0002_1_1#0', i) * 2.5 * 4 / 15 / 720
            c[8] = c[7] * 1080 / 1920
            c[11] = v('ADBE_Optics_Compensation_0001_1_4#0', i) / 100
            c[9] = v('ADBE_IE_6bef21f3d8718ceb19fdaaf-0001_1_2#0', i) / 2.2
        elif slug in ['flash_in', 'flash_out']:
            c[2] = c[3] = v('ADBE_Tile_0002_0_0#0', i, 100) / 100
            x = 'ADBE_Gaussian_Blur_2_0001_0_' + ('3' if slug == 'flash_in' else '2') + '#0'
            y = 'ADBE_Gaussian_Blur_2_0001_0_' + ('4' if slug == 'flash_in' else '3') + '#0'
            ex = 'ADBE_Exposure2_0003_0_' + ('5' if slug == 'flash_in' else '4') + '#0'
            c[7] = v(x, i) / 8 * 0.6 * 1.5 / 1080
            c[8] = v(y, i) / 8 * 0.6 * 1.5 / 1920
            c[9] = v(ex, i) / 2.2
        for k, x in enumerate(c):
            cols[k][i] = round(x, 8)
        if slug == 'stretch_in':
            cols[21][i] = v('LumiDeepGlow_41-effect1#exposure#number#0', i)
    for k, prev in [(0, 16), (1, 17), (2, 18), (3, 19), (4, 20)]:
        cols[prev] = cols[k].copy()
    if slug in ['stretch_in', 'stretch_out', 'motion_slide']:
        for k, prev in [(0, 16), (1, 17), (2, 18), (3, 19), (4, 20)]:
            step = round(1 / 30 / (a['duration'] / 1000000.0) * (N - 1))
            cols[prev] = [cols[k][max(0, i - step)] for i in range(N)]
    dest = root / slug
    dest.mkdir(parents=True, exist_ok=True)
    manifest = {'capcut_effect_id': a['id'], 'kind': 'animation', 'name': {'en': en, 'it': a['name']}, 'category': {'in': 'in', 'out': 'out', 'group': 'combo'}[a['type']], 'default_secs': a['duration'] / 1000000.0, 'curves': [col if len(set(col)) > 1 else col[:1] for col in cols]}
    code = shader
    if slug in ['zoom_2', 'swing_top_left', 'bounce_1', 'zoom_1']:
        code = code.replace('const MIRROR: bool = false;', 'const MIRROR: bool = true;').replace('const EXTENDED: bool = false;', 'const EXTENDED: bool = true;')
    if slug in ['side_slide', 'motion_slide', 'flash_in', 'flash_out', 'slide_up']:
        code = code.replace('const NATIVE: bool = false;', 'const NATIVE: bool = true;').replace('const MIRROR: bool = false;', 'const MIRROR: bool = true;')
    if slug == 'motion_slide':
        for k, prev in [(0, 16), (1, 17), (2, 18), (3, 19), (4, 20)]:
            step = round(1.66 / 30 / (a['duration'] / 1000000.0) * (N - 1))
            cols[prev] = [cols[k][min(N - 1, i + step)] for i in range(N)]
        code = code.replace('0.75 + 0.5 * fract(t * 17.0)', 't')
    if slug == 'distort_stretch':
        code = (scripts / 'distort_stretch.wgsl').read_text()
        cols = [b[k] for k in ['u_TwistRotFactor', 'u_TwistFactor', 'u_ScaleFactor', 'u_Distort', 'dirBlurStep', 'scaleBlurStep']]
        # Twist orientation fitted to CapCut's reference.
        cols[0] = [-math.radians(x) - math.pi / 3 for x in cols[0]]
    manifest['curves'] = [col if len(set(col)) > 1 else col[:1] for col in cols]
    (dest / 'manifest.json').write_text(json.dumps(manifest, separators=(',', ':')) + '\n')
    (dest / 'shader.wgsl').write_text(code)
