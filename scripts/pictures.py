"""Builds the showcase's picture folder from the engine's levels page
(decision 6: the pictures stay out of git and are copied at deploy).

    python3 -I scripts/pictures.py [ENGINE_WEB_FOLDER] [OUT_FOLDER]

ENGINE_WEB_FOLDER is the engine's tmp/image/web (its levels.html and the
conversion's out-levels75-* folder); OUT_FOLDER (tmp/pictures) receives
captions.tsv, one line per picture (name, group, caption with its
attribution), and each picture's NAME_l15.tti, NAME_l25.tti and
NAME_l35.tti.
"""
import json
import re
import shutil
import sys
from pathlib import Path

LEVELS = ("l15", "l25", "l35")

def main():
    web = Path(sys.argv[1] if len(sys.argv) > 1 else Path.home() / "devel/neoteletext-engine/tmp/image/web")
    out = Path(sys.argv[2] if len(sys.argv) > 2 else "tmp/pictures")
    page = (web / "levels.html").read_text()
    script = max(re.findall(r"<script[^>]*>(.*?)</script>", page, flags=re.S), key=len)
    data = json.loads(re.search(r"const DATA = (\[.*?\]);", script, flags=re.S).group(1))
    conversions = sorted(web.glob("out-levels75-*"))
    if not conversions:
        sys.exit(f"no out-levels75-* folder in {web}")
    source = conversions[-1]
    out.mkdir(parents=True, exist_ok=True)
    lines = []
    for picture in data:
        name, group, caption = picture["n"], picture["g"], picture["c"]
        copied = 0
        for level in LEVELS:
            tti = source / f"{name}_{level}.tti"
            if tti.exists():
                shutil.copyfile(tti, out / tti.name)
                copied += 1
        if copied:
            clean = " ".join(caption.replace("\t", " ").split())
            lines.append(f"{name}\t{group}\t{clean}")
    (out / "captions.tsv").write_text("\n".join(lines) + "\n")
    print(f"{len(lines)} pictures from {source.name} into {out}")

main()
