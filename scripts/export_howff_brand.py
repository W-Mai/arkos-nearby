#!/usr/bin/env python3
"""Export Howff SVG identity assets from the application's mirui scene."""

import argparse
from html import escape
import importlib.util
import json
from pathlib import Path
import tempfile

import build_howff_gui as gui

ROOT = Path(__file__).resolve().parents[1]
LOGO_SIZE = 48
WORDMARK_GAP = 12
WORDMARK_SCALE = 2
# Visible bounds of the bundled 24px pixel wordmark, including scene placement.
WORDMARK_INK = (0, 7, 58, 16)


def renderer_module():
    source = ROOT / "gui/nearby/examples/dual_demo/svg.py"
    spec = importlib.util.spec_from_file_location("howff_svg", source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def documents(scene):
    renderer = renderer_module().Svg(
        {"frames": [{}], "fps": 1, "definitions": scene["definitions"]}
    )
    renderer.resources()
    logo = renderer.children([[node] for node in scene["logo"]])
    text_nodes = [
        node
        for node in scene["name"]
        if node["tag"] == "use" and scene["definitions"][node["ref"]]["kind"] == "text"
    ]
    name = renderer.children([[node | {"fill": "currentColor"}] for node in text_nodes])
    definitions = "".join(renderer.defs)
    title = escape(scene["brand"])
    ink_x, ink_y, ink_width, ink_height = WORDMARK_INK
    name_x = LOGO_SIZE + WORDMARK_GAP - ink_x * WORDMARK_SCALE
    name_y = (LOGO_SIZE - ink_height * WORDMARK_SCALE) // 2 - ink_y * WORDMARK_SCALE
    width = LOGO_SIZE + WORDMARK_GAP + ink_width * WORDMARK_SCALE

    def document(width, body, style=""):
        return (
            '<svg xmlns="http://www.w3.org/2000/svg" '
            f'width="{width}" height="{LOGO_SIZE}" viewBox="0 0 {width} {LOGO_SIZE}" '
            'role="img" aria-labelledby="title">'
            f'<title id="title">{title}</title>{style}'
            f"<defs>{definitions}</defs>{body}</svg>\n"
        )

    return {
        "logo.svg": document(LOGO_SIZE, logo),
        "wordmark.svg": document(
            width,
            logo
            + f'<g class="name" transform="translate({name_x} {name_y}) scale({WORDMARK_SCALE})">'
            + name
            + "</g>",
            "<style>.name{color:#42664c}@media(prefers-color-scheme:dark){.name{color:#f2eddb}}</style>",
        ),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "docs/assets",
    )
    args = parser.parse_args()
    cargo, environment = gui.toolchain("1.97.0")
    with tempfile.TemporaryDirectory(prefix="howff-brand-scenes-") as temporary:
        scene_path = Path(temporary) / "identity.json"
        gui.run(
            [
                str(cargo),
                "run",
                "--manifest-path",
                str(gui.MANIFEST),
                "--offline",
                "--locked",
                "--example",
                "brand_assets",
                "--",
                str(scene_path),
            ],
            environment,
            cwd=ROOT,
        )
        scene = json.loads(scene_path.read_text())
    args.output.mkdir(parents=True, exist_ok=True)
    for name, content in documents(scene).items():
        (args.output / name).write_text(content)
    print(args.output)


if __name__ == "__main__":
    main()
