#!/usr/bin/env python3
"""Compile captured mirui scenes into a self-contained animated SVG."""

import argparse
from collections import Counter
from difflib import SequenceMatcher
from html import escape
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "scripts"))
import build_nearby_gui as gui  # noqa: E402


def num(value):
    return f"{value:.5f}".rstrip("0").rstrip(".") if value else "0"


def attr(value):
    if isinstance(value, (int, float)):
        return num(value)
    return escape(str(value), quote=True)


def attributes(values):
    return " ".join(f'{key}="{attr(value)}"' for key, value in values.items())


def matrix(value):
    return f"matrix({','.join(num(v) for v in value)})"


def clip_contains_rect(node):
    """Prove that clipping cannot affect this rectangle, including its edge."""
    if node["tag"] != "rect" or "clip" not in node:
        return False
    a, b, c, d, tx, ty = node.get("transform", [1, 0, 0, 1, 0, 0])
    stroke = node.get("stroke-width", 0) / 2
    left, top = node["x"] - stroke, node["y"] - stroke
    right = node["x"] + node["width"] + stroke
    bottom = node["y"] + node["height"] + stroke
    corners = [
        (a * x + c * y + tx, b * x + d * y + ty)
        for x in (left, right)
        for y in (top, bottom)
    ]
    x, y, width, height = node["clip"]
    # Retain one device pixel for antialias coverage on every edge.
    return all(
        x + 1 <= px <= x + width - 1 and y + 1 <= py <= y + height - 1
        for px, py in corners
    )


def simplify(values, tolerance):
    """Bound the maximum interpolation error at every original sample."""
    if len(values) < 3:
        return list(range(len(values)))
    keep = {0, len(values) - 1}
    pending = [(0, len(values) - 1)]
    while pending:
        first, last = pending.pop()
        worst, error = first, 0
        for index in range(first + 1, last):
            progress = (index - first) / (last - first)
            deviation = max(
                abs(v - (a + (b - a) * progress)) / bound
                for v, a, b, bound in zip(
                    values[index], values[first], values[last], tolerance
                )
            )
            if deviation > error:
                worst, error = index, deviation
        if error > 1:
            keep.add(worst)
            pending.extend([(first, worst), (worst, last)])
    return sorted(keep)


class Svg:
    def __init__(self, capture):
        self.capture = capture
        self.count = len(capture["frames"])
        self.duration = self.count / capture["fps"]
        self.styles = {}
        self.combinations = {}
        self.defs = []
        self.clip_keys = {}
        self.stats = Counter()

    def css(self, body):
        if body not in self.styles:
            self.styles[body] = f"a{len(self.styles)}"
        return self.styles[body]

    def classes(self, tracks):
        if len(tracks) == 1:
            return tracks[0]
        key = tuple(tracks)
        if key not in self.combinations:
            self.combinations[key] = f"b{len(self.combinations)}"
        return self.combinations[key]

    def gate(self, active):
        changes = [0] + [i for i in range(1, self.count) if active[i] != active[i - 1]]
        return self.css(
            "".join(
                f"{num(i * 100 / self.count)}%{{visibility:{'visible' if active[i] else 'hidden'};animation-timing-function:steps(1,end)}}"
                for i in changes
            )
            + f"100%{{visibility:{'visible' if active[-1] else 'hidden'}}}"
        )

    def track(self, name, values, start):
        if all(v == values[0] for v in values):
            return "", None
        self.stats["animated_attributes"] += 1
        if name == "transform":
            # Matrix coefficients need a tighter bound than translations.
            # At the 1120px canvas edge this keeps the error below 0.2px.
            indices = simplify(values, [0.00008] * 4 + [0.08, 0.08])
            frames = {0: values[0]}
            frames.update({(start + i) / self.count: values[i] for i in indices})
            frames[1] = values[-1]
            body = "".join(
                f"{num(t * 100)}%{{transform:{matrix(v)}}}"
                for t, v in sorted(frames.items())
            )
            return "", self.css(body)
        numeric = isinstance(values[0], (int, float))
        if numeric:
            indices = simplify(
                [[v] for v in values], [0.001 if name == "opacity" else 0.08]
            )
        else:
            indices = [0] + [
                i for i in range(1, len(values)) if values[i] != values[i - 1]
            ]
        frames = {0: values[0]}
        frames.update({(start + i) / self.count: values[i] for i in indices})
        frames[1] = values[-1]
        times = sorted(frames)
        if name not in (
            "x",
            "y",
            "width",
            "height",
            "rx",
            "opacity",
            "fill",
            "stroke",
            "stroke-width",
        ):
            raise ValueError(f"Unsupported animated SVG property: {name}")
        unit = "px" if numeric and name != "opacity" else ""
        easing = "" if numeric else ";animation-timing-function:steps(1,end)"
        body = "".join(
            f"{num(t * 100)}%{{{name}:{attr(frames[t])}{unit}{easing}}}" for t in times
        )
        return "", self.css(body)

    def clip(self, values, start):
        key = json.dumps([start, values], separators=(",", ":"))
        if all(v == values[0] for v in values):
            key = json.dumps(values[0])
        if key in self.clip_keys:
            return self.clip_keys[key]
        identity = f"c{len(self.clip_keys)}"
        self.clip_keys[key] = identity
        attrs = dict(zip(("x", "y", "width", "height"), values[0]))
        animations = []
        tracks = []
        for index, name in enumerate(attrs):
            animation, css = self.track(name, [v[index] for v in values], start)
            animations.append(animation)
            if css:
                tracks.append(css)
        if tracks:
            attrs["class"] = self.classes(tracks)
        self.defs.append(
            f'<clipPath id="{identity}" clipPathUnits="userSpaceOnUse">'
            f"<rect {attributes(attrs)}>{''.join(animations)}</rect></clipPath>"
        )
        return identity

    def node(self, nodes, start):
        first = nodes[0]
        tag = first["tag"]
        attrs = {k: v for k, v in first.items() if k not in ("tag", "clip", "ref")}
        body = []
        tracks = []
        for key in attrs:
            animation, css = self.track(key, [n[key] for n in nodes], start)
            body.append(animation)
            if css:
                tracks.append(css)
        if "transform" in attrs:
            attrs["transform"] = matrix(attrs["transform"])
        if tracks:
            attrs["class"] = self.classes(tracks)
        if tag == "use":
            attrs["href"] = f"#d{first['ref']}"
        if tag == "screen":
            # A viewport nests the actual captured menu scene at native size.
            tag = "g"
            viewport = {k: attrs.pop(k) for k in ("x", "y", "width", "height")}
            viewport["viewBox"] = "0 0 640 480"
            body.append(
                f'<svg {attributes(viewport)}><use href="#{first["ref"]}"/></svg>'
            )
        content = f"<{tag} {attributes(attrs)}>{''.join(body)}</{tag}>"
        if "clip" in first:
            clip = self.clip([n["clip"] for n in nodes], start)
            return f'<g clip-path="url(#{clip})">{content}</g>'
        return content

    def children(self, children):
        """Batch adjacent static, disjoint rectangles with identical paint."""
        output, batch = [], []

        def flush():
            if len(batch) == 1:
                output.append(self.node([batch[0]] * self.count, 0))
            elif batch:
                data = "".join(
                    f"M{num(n['x'])} {num(n['y'])}h{num(n['width'])}"
                    f"v{num(n['height'])}h{num(-n['width'])}Z"
                    for n in batch
                )
                output.append(f'<path d="{data}"/>')
                self.stats["batched_rectangles"] += len(batch)
                self.stats["rectangle_batches"] += 1
            batch.clear()

        for samples in children:
            node = samples[0]
            eligible = (
                node["tag"] == "rect"
                and set(node) <= {"tag", "x", "y", "width", "height", "rx"}
                and node.get("rx", 0) == 0
                and all(n == node for n in samples)
            )
            if not eligible:
                flush()
                output.append(self.node(samples, 0))
                continue
            # Keep overlaps as separate draws: combining translucent fills
            # would change their coverage and painter-order semantics.
            if any(
                node["x"] < n["x"] + n["width"]
                and n["x"] < node["x"] + node["width"]
                and node["y"] < n["y"] + n["height"]
                and n["y"] < node["y"] + node["height"]
                for n in batch
            ):
                flush()
            batch.append(node)
        flush()
        return "".join(output)

    def layer(self, name):
        frames = [f[name] for f in self.capture["frames"]]

        def signature(node):
            geometry = (
                tuple(node.get(k) for k in ("x", "y", "width", "height", "rx"))
                if node["tag"] == "rect"
                else ()
            )
            return node["tag"], node.get("ref"), tuple(sorted(node)), geometry

        # Keep an ordered supersequence of draw slots. Every sampled frame is
        # exactly a subsequence; inserted/removed draws only toggle visibility.
        # This retains painter order without duplicating entire page scenes.
        slots = []
        previous_keys = None
        previous_slots = []
        for index, frame in enumerate(frames):
            keys = [signature(n) for n in frame]
            if keys == previous_keys:
                for slot, node in zip(previous_slots, frame):
                    slot[1][index] = node
                continue
            matcher = SequenceMatcher(None, [s[0] for s in slots], keys)
            merged = []
            for action, a, b, c, d in matcher.get_opcodes():
                if action == "equal":
                    for slot, node in zip(slots[a:b], frame[c:d]):
                        slot[1][index] = node
                        merged.append(slot)
                else:
                    merged.extend(slots[a:b])
                    for key, node in zip(keys[c:d], frame[c:d]):
                        samples = [None] * self.count
                        samples[index] = node
                        merged.append((key, samples))
            slots = merged
            previous_keys = keys
            previous_slots = [s for s in slots if s[1][index] is not None]
        for index, frame in enumerate(frames):
            reconstructed = [s[1][index] for s in slots if s[1][index] is not None]
            if reconstructed != frame:
                raise ValueError(f"Painter order changed in {name} frame {index}")
        self.stats[f"{name}_draw_slots"] = len(slots)
        groups = []
        for _, samples in slots:
            active = [n is not None for n in samples]
            last = next(n for n in samples if n is not None)
            filled = []
            for node in samples:
                if node is not None:
                    last = node
                filled.append(last)
            if all(clip_contains_rect(n) for n in filled):
                filled = [{k: v for k, v in n.items() if k != "clip"} for n in filled]
                self.stats["unclipped_rectangles"] += 1
            common = [
                {
                    k: n[k]
                    for k in ("transform", "clip", "opacity", "fill", "stroke")
                    if k in n
                }
                for n in filled
            ]
            children = [
                {k: v for k, v in n.items() if k not in common[0]} for n in filled
            ]
            if groups and groups[-1][0] == active and groups[-1][1] == common:
                groups[-1][2].append(children)
            else:
                groups.append((active, common, [children]))
        self.stats[f"{name}_paint_groups"] = len(groups)
        output = []
        visibility_groups = []
        for active, common, children in groups:
            wrapper = self.node([dict(n, tag="g") for n in common], 0)
            body = self.children(children)
            # The wrapper's inner group owns the shared paint and animation.
            closing = wrapper.index("</g>")
            body = wrapper[:closing] + body + wrapper[closing:]
            if visibility_groups and visibility_groups[-1][0] == active:
                visibility_groups[-1][1].append(body)
            else:
                visibility_groups.append((active, [body]))
        self.stats[f"{name}_visibility_groups"] = len(visibility_groups)
        for active, bodies in visibility_groups:
            body = "".join(bodies)
            if not all(active):
                # Hide inactive draw slots without stopping their shared
                # document-time animations, including inside use trees.
                # Firefox supports visibility keyframes but not display ones.
                body = f'<g visibility="visible" class="{self.gate(active)}">{body}</g>'
            output.append(body)
        return "".join(output)

    def resources(self):
        for index, item in enumerate(self.capture["definitions"]):
            kind = item["kind"]
            if kind == "path":
                body = f"<path {attributes({k: v for k, v in item.items() if k != 'kind'})}/>"
            elif kind == "glyph":
                body = "".join(f"<path {attributes(p)}/>" for p in item["paths"])
            elif kind == "text":
                body = "".join(
                    f'<use href="#d{g["ref"]}" transform="translate({num(g["x"])} {num(g["y"])}) scale({num(g["scale"])})"/>'
                    for g in item["glyphs"]
                )
            else:
                raise ValueError(f"Unknown definition: {kind}")
            self.defs.append(f'<g id="d{index}">{body}</g>')

    def build(self):
        self.resources()
        host, guest, stage = (self.layer(name) for name in ("host", "guest", "stage"))
        css = "".join(
            f".{name}{{animation:{name} {num(self.duration)}s linear infinite}}@keyframes {name}{{{body}}}"
            for body, name in self.styles.items()
        )
        css += "".join(
            f".{name}{{animation:"
            + ",".join(
                f"{track} {num(self.duration)}s linear infinite" for track in tracks
            )
            + "}"
            for tracks, name in self.combinations.items()
        )
        self.stats["shared_shapes"] = len(self.capture["definitions"])
        self.stats["shared_css_tracks"] = len(self.styles)
        return (
            '<svg xmlns="http://www.w3.org/2000/svg" '
            f'viewBox="0 0 {self.capture["width"]} {self.capture["height"]}" '
            'width="840" height="585" role="img" aria-labelledby="title">'
            '<title id="title">附近联机 · 双机演示</title>'
            f"<style>{css}</style><defs>{''.join(self.defs)}"
            f'<g id="screen-0">{host}</g><g id="screen-1">{guest}</g>'
            f"</defs>{stage}</svg>"
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--frames", type=Path, default=Path("/tmp/arkos-nearby-svg"))
    parser.add_argument("--reuse-scenes", action="store_true")
    parser.add_argument("--toolchain", default="1.92")
    args = parser.parse_args()
    if args.output.suffix.lower() != ".svg":
        parser.error("--output must use the .svg extension")
    if not args.reuse_scenes:
        cargo, env = gui.toolchain(args.toolchain)
        env["CARGO_INCREMENTAL"] = "0"
        subprocess.run(
            [
                str(cargo),
                "build",
                "--manifest-path",
                str(gui.MANIFEST),
                "--release",
                "--example",
                "dual_demo",
                "--locked",
                "--offline",
            ],
            check=True,
            env=env,
            cwd=ROOT,
        )
        binary = Path(env["CARGO_TARGET_DIR"]) / "release/examples/dual_demo"
        subprocess.run([str(binary), str(args.frames.resolve()), "--scene"], check=True)
    capture = json.loads((args.frames / "scenes.json").read_text())
    svg = Svg(capture)
    document = svg.build()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.output.with_suffix(".part.svg")
    temporary.write_text(document)
    temporary.replace(args.output)
    print(
        json.dumps(
            {"bytes": args.output.stat().st_size, "seconds": svg.duration, **svg.stats},
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
