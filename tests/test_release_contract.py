"""Check distribution identities and display-only public assets."""

import hashlib
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


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
        self.assertEqual(manifest["gui_version"], "0.47.0")

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
