#!/usr/bin/env python3
"""Regenerates the sticker catalogs in crates/vv-app/assets/stickers/.

Only names are stored; the images are downloaded when the user needs them.
Needs network access (GitHub API and raw.githubusercontent.com).
"""
import json
import pathlib
import re
import urllib.request

OUT = pathlib.Path(__file__).resolve().parents[2] / "crates/vv-app/assets/stickers"
ORDERING = "https://raw.githubusercontent.com/googlefonts/emoji-metadata/main/emoji_16_0_ordering.json"


def fetch(url):
    with urllib.request.urlopen(url, timeout=60) as response:
        return response.read()


def tree(repo):
    url = f"https://api.github.com/repos/{repo}/git/trees/main?recursive=1"
    return [e["path"] for e in json.loads(fetch(url))["tree"]]


def emoji():
    have = {
        p.split("/")[-1][len("emoji_u"):-len(".png")]
        for p in tree("googlefonts/noto-emoji")
        if p.startswith("3D/png/512/")
    }
    rows = []
    seen = set()
    for group, entry in enumerate(json.loads(fetch(ORDERING))):
        for item in entry["emoji"]:
            code = "_".join(f"{c:x}" for c in item["base"])
            if code not in have or code in seen:
                continue
            seen.add(code)
            short = (item["shortcodes"] or [":" + code + ":"])[0].strip(":")
            rows.append(f"{code}\t{short}\t{group}")
    (OUT / "emoji.tsv").write_text("\n".join(rows) + "\n")
    print("emoji", len(rows))


def lucide():
    names = sorted(
        p[len("icons/"):-len(".svg")]
        for p in tree("lucide-icons/lucide")
        if p.startswith("icons/") and p.endswith(".svg")
    )
    (OUT / "lucide.txt").write_text("\n".join(names) + "\n")
    print("lucide", len(names))


def phosphor():
    names = sorted(
        p[len("assets/fill/"):-len("-fill.svg")]
        for p in tree("phosphor-icons/core")
        if re.fullmatch(r"assets/fill/[a-z0-9-]+-fill\.svg", p)
    )
    (OUT / "phosphor.txt").write_text("\n".join(names) + "\n")
    print("phosphor", len(names))


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    emoji()
    lucide()
    phosphor()
