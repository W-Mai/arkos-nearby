"""Check distribution identities and display-only public assets."""

import hashlib
import json
from pathlib import Path
import sys
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import build_nearby_gui as gui  # noqa: E402


class ReleaseContract(unittest.TestCase):
    def test_copied_application_sources_match_the_recorded_development_snapshot(self):
        record = json.loads((ROOT / "release/provenance.json").read_text())
        for name, expected in record["source_files"].items():
            with self.subTest(name=name):
                self.assertTrue(
                    name.startswith(
                        (
                            "native/nearby/",
                            "native/core_inspect/",
                            "native/core_probe/",
                            "native/netplay_compression/",
                            "native/owned_cores/",
                            "gui/nearby/",
                        )
                    )
                )
                self.assertEqual(
                    hashlib.sha256((ROOT / name).read_bytes()).hexdigest(), expected
                )

    def test_native_asset_manifest_has_unique_confined_names_and_contiguous_offsets(
        self,
    ):
        manifest = json.loads((ROOT / "release/manifest.json").read_text())
        seen = set()
        offset = 0
        for entry in manifest["files"]:
            name = Path(entry["name"])
            self.assertFalse(name.is_absolute())
            self.assertNotIn("..", name.parts)
            self.assertNotIn(entry["name"], seen)
            self.assertEqual(entry["offset"], offset)
            self.assertIn(entry["mode"], (0o644, 0o755))
            seen.add(entry["name"])
            offset += entry["size"]
        self.assertEqual(manifest["gui_version"], gui.locked_dependencies()["mirui"])

    def test_renderer_sources_and_licenses_match_the_locked_dependencies(self):
        versions = gui.locked_dependencies()
        source = json.loads((ROOT / "release/release-sources.json").read_text())[
            "renderer"
        ]
        self.assertEqual(source["version"], versions["mirui"])
        self.assertEqual(source["mirx_version"], versions["mirx"])
        self.assertEqual(source["source_sha256"], gui.source_sha256())
        for name, value in gui.patch_record()[1].items():
            self.assertEqual(source[name], value)
        lock = ROOT / "gui/nearby/Cargo.lock"
        self.assertEqual(
            source["cargo_lock_sha256"], hashlib.sha256(lock.read_bytes()).hexdigest()
        )
        manifest = json.loads((ROOT / "release/manifest.json").read_text())
        renderer = next(
            entry for entry in manifest["files"] if entry["name"] == "arkos-nearby-gui"
        )
        self.assertEqual(source["sha256"], renderer["sha256"])
        dependencies = json.loads((ROOT / "release/rust-dependencies.json").read_text())
        for package in tomllib.loads(lock.read_text())["package"]:
            if package["name"] not in {"mirui", "mirui-macros", "mirx"}:
                continue
            records = [
                entry
                for entry in dependencies
                if entry["name"] == package["name"]
                and entry["version"] == package["version"]
            ]
            self.assertEqual(len(records), 1)
            record = records[0]
            self.assertEqual(record["checksum"], package["checksum"])
            self.assertTrue(record["included"])
            for name in record["license_files"]:
                path = Path(name)
                self.assertFalse(path.is_absolute())
                self.assertNotIn("..", path.parts)
                self.assertTrue((ROOT / path).is_file())

    def test_game_probe_is_a_separate_executable_asset_with_build_metadata(self):
        manifest = json.loads((ROOT / "release/manifest.json").read_text())
        files = {entry["name"]: entry for entry in manifest["files"]}
        self.assertEqual(files["core-probe64"]["mode"], 0o755)
        self.assertEqual(files["core-probe-build.json"]["mode"], 0o644)
        self.assertEqual(files["licenses/core-probe-ABI-LICENSE.txt"]["mode"], 0o644)
        self.assertEqual(
            (ROOT / "docs/licenses/core-probe-ABI-LICENSE.txt").read_bytes(),
            (ROOT / "native/core_probe/ABI-LICENSE.txt").read_bytes(),
        )

    def test_documented_previews_use_the_installed_renderer_schema(self):
        for path in sorted((ROOT / "docs/screenshots/views").glob("*.json")):
            view = json.loads(path.read_text())
            self.assertIn(view["page"], {"games", "choice", "rooms", "host", "joined"})
            self.assertNotIn("path", view)
            for row in view.get("entries", []) + view.get("rooms", []):
                self.assertNotIn("path", row)


if __name__ == "__main__":
    unittest.main()
