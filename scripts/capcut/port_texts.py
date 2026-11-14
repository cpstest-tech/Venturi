#!/usr/bin/env python3
"""Port the text reference draft and its cached fonts into editable templates."""

import argparse
import copy
import json
import math
from pathlib import Path, PureWindowsPath
import shutil
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / 'crates/vv-app/assets'
NAMES = [
    ('capcut_pause', 'Pause', 'social'),
    ('capcut_onfire', 'On Fire', 'social'),
    ('capcut_modern', 'Modern Halo', 'titles'),
    ('capcut_wide', 'Wide', 'titles'),
    ('capcut_blue', 'Blue Outline', 'titles'),
    ('capcut_cracked', 'Cracked Glow', 'titles'),
    ('capcut_purple', 'Purple Glow', 'titles'),
]


def color(fill):
    content = fill.get('content', fill)
    rgb = content.get('solid', {}).get('color', [1, 1, 1])
    alpha = fill.get('alpha', 1) * content.get('solid', {}).get('alpha', 1)
    return '#' + ''.join(f'{round(max(0, min(1, v)) * 255):02X}' for v in [*rgb, alpha])


def fill(layer, source):
    layer['color'] = color(source)
    content = source.get('content', {})
    if content.get('render_type') == 'gradient':
        gradient = content['gradient']
        layer['fill'] = {
            'angle': gradient['angle'],
            'stops': sorted([
                [p, color({'solid': {'color': rgb, 'alpha': a}})]
                for p, rgb, a in zip(gradient['percent'], gradient['color'], gradient['alpha'])
            ]),
        }


def strokes(source, size):
    return [dict(color=color(s), width=s['width'] * size)
            for s in source if s.get('enable', True) and s.get('alpha', 1) > 0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('draft', type=Path)
    parser.add_argument('windows_user', type=Path)
    args = parser.parse_args()
    draft = json.loads(args.draft.read_text())
    materials = draft['materials']
    texts = {t['id']: t for t in materials['texts']}
    templates = {t['id']: t for t in materials['text_templates']}
    segments = [s for t in draft['tracks'] if t['type'] == 'text' for s in t['segments']]
    segments.sort(key=lambda s: s['target_timerange']['start'])
    if len(segments) != 8:
        raise ValueError('Expected the seven-style text reference followed by its subtitle sample')
    font_dir = ASSETS / 'fonts/capcut'
    font_dir.mkdir(exist_ok=True)
    font_files = {}

    def local(path):
        return args.windows_user / PureWindowsPath(path).relative_to('C:/Users/Utente')

    def font(path):
        source = local(path)
        target = font_dir / source.name
        shutil.copyfile(source, target)
        family = subprocess.check_output(
            ['fc-scan', '--format', '%{family[0]}', str(target)], text=True)
        font_files[target.name] = family
        return family

    scale = 1080 / draft['canvas_config']['height']
    for segment, (identifier, name, category) in zip(segments, NAMES):
        template = templates.get(segment['material_id'])
        attachment = template['text_info_resources'][0] if template else None
        material = texts[attachment['text_material_id'] if attachment else segment['material_id']]
        content = json.loads(material['content'])
        style = content['styles'][0]
        clip_scale = segment['clip']['scale']['y']
        if attachment:
            clip_scale *= attachment['attach_info']['clip']['scale']['y']
        size = style['size'] * (350 / 60) * clip_scale * scale
        family = font(style['font']['path'])
        base = dict(text=content['text'], font=family, size=size, color='#FFFFFF',
                    line_spacing=-size * 0.03,
                    position=[segment['clip']['transform']['x'] * 540 * scale,
                              segment['clip']['transform']['y'] * 960 * scale])
        if attachment:
            transform = attachment['attach_info']['clip']['transform']
            outer_scale = segment['clip']['scale']['y'] * scale
            base['position'][0] += transform['x'] * outer_scale
            base['position'][1] += transform['y'] * outer_scale
        font_bytes = local(style['font']['path']).read_bytes()
        for table in range(struct.unpack_from('>H', font_bytes, 4)[0]):
            tag, _, offset, _ = struct.unpack_from('>4sIII', font_bytes, 12 + table * 16)
            if tag == b'OS/2':
                base['weight'] = struct.unpack_from('>H', font_bytes, offset + 4)[0]
                base['italic'] = bool(struct.unpack_from('>H', font_bytes, offset + 62)[0] & 1)
        layers = []
        effect = style.get('effectStyle')
        if effect and (local(effect['path']) / 'effectStyle.json').exists():
            effect_style = json.loads((local(effect['path']) / 'effectStyle.json').read_text())
            fill(base, effect_style['fill'])
            for shadow in reversed(effect_style.get('shadows', [])):
                if not shadow.get('enable', True) or shadow.get('alpha', 1) <= 0:
                    continue
                layer = copy.deepcopy(base)
                layer.pop('fill', None)
                angle = math.radians(shadow.get('angle', -45))
                distance = shadow.get('distance', 0) * clip_scale * scale
                layer['position'] = [base['position'][0] + math.cos(angle) * distance,
                                     base['position'][1] + math.sin(angle) * distance]
                fill(layer, shadow)
                layer['strokes'] = strokes(shadow.get('strokes', []), size)
                blur = shadow.get('diffuse', shadow.get('feather', 0)) * size * 2
                if blur > 0:
                    layer['shadows'] = [dict(color=color(shadow), offset=[0, 0],
                                             blur=blur, opacity=100)]
                if shadow.get('thickness_projection_enable'):
                    layer['extrude'] = dict(color=color(shadow),
                        depth=shadow['thickness_projection_distance'] * clip_scale * scale,
                        angle=shadow['thickness_projection_angle'])
                layers.append(layer)
            base['strokes'] = strokes(effect_style.get('strokes', []), size)
        else:
            fill(base, style['fill'])
            base['shadows'] = []
            for shadow in style.get('shadows', []):
                angle = math.radians(shadow['angle'])
                distance = shadow['distance'] * clip_scale * scale
                base['shadows'].append(dict(color=color(shadow),
                    offset=[math.cos(angle) * distance, math.sin(angle) * distance],
                    blur=shadow.get('diffuse', 0) * size * 2, opacity=100))
        layers.append(base)
        if identifier == 'capcut_pause':
            base['color'] = '#FFFFFF'
            base['glows'] = [dict(color='#FEE400', radius=size * 0.18, intensity=1.5)]
        if identifier == 'capcut_onfire':
            base['glows'] = [dict(color='#E90300', radius=size * 0.18, intensity=1.5)]
            layers[-2]['strokes'] = [
                dict(color='#F2C992', width=size * 0.10),
                dict(color='#FB140B', width=size * 0.13),
            ]
        if identifier == 'capcut_cracked':
            base['glows'] = [dict(color='#F5E746', radius=size * 0.16, intensity=2)]
        if identifier == 'capcut_purple':
            base['glows'] = [dict(color='#AA00FF', radius=size * 0.12, intensity=1.5)]
        if identifier == 'capcut_pause':
            resource = next(r for r in template['resources'] if r['panel'] == 'text')
            curves = json.loads(subprocess.check_output([
                'lua', str(ROOT / 'scripts/capcut/bake_text.lua'),
                str(local(resource['path']) / 'lua/src'), '401'], text=True))
            enter = dict(id='capcut_pause', duration=1.616666, stagger=8 / 31,
                order='Forward', granularity='Char', easing='Linear', tracks=[],
                sampled_tracks=[dict(prop='Scale', values=[v / 100 for v in curves['scale']])],
                tint=dict(r=1, g=1, b=1, a=1), reveal_dir='Right')
            for layer in layers:
                layer['anim'] = dict(enter_custom=enter)
        if identifier == 'capcut_onfire':
            for layer in layers:
                layer['anim'] = {'in': dict(preset='mask_reveal', duration=0.95,
                                           granularity='block')}
            resource = next(r for r in template['resources'] if r['panel'] == 'default')
            atlas = next(local(resource['path']).glob('*/*.json'))
            output = ROOT / 'crates/vv-render/assets/text-stickers'
            output.mkdir(exist_ok=True)
            shutil.copyfile(atlas, output / 'onfire.json')
            shutil.copyfile(atlas.with_suffix('.png'), output / 'onfire.png')
            info = template['non_text_info_resources'][0]
            attached = info['attach_info']
            transform = attached['clip']
            base['sticker'] = dict(asset='capcut_onfire',
                size=[attached['original_size_width'] * transform['scale']['x'] * scale / size,
                      attached['original_size_height'] * transform['scale']['y'] * scale / size],
                offset=[(transform['transform']['x'] * scale - base['position'][0]) / size,
                        (transform['transform']['y'] * scale - base['position'][1]) / size],
                rotation=transform['rotation'], start=attached['start_time'] / 1e6)
        if len(layers) > 1 and 'glows' in base:
            layers[0]['glows'] = base.pop('glows')
        base['underlays'] = layers[:-1]
        for layer in base['underlays']:
            layer.pop('anim', None)
        definition = dict(version=1, id=identifier, name=name, category=category,
                          tags=['capcut', 'ported', name.lower()], duration=3, layers=[base])
        (ASSETS / f'text-templates/{identifier}.json').write_text(
            json.dumps(definition, indent=2) + '\n')

    subtitle = texts[templates[segments[-1]['material_id']]['text_info_resources'][0]['text_material_id']]
    subtitle_font = font(json.loads(subtitle['content'])['styles'][0]['font']['path'])
    assert subtitle_font == 'ZY Resolve', subtitle_font
    (font_dir / 'sources.json').write_text(json.dumps(font_files, indent=2) + '\n')


if __name__ == '__main__':
    main()
