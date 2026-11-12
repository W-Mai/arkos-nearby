"""A prepared GPU renderer must survive native source builds and stale inputs."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location(
    "release_builder", ROOT / "scripts/build.py"
)
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class RendererBoundary(unittest.TestCase):
    def test_verified_gpu_is_reused_without_starting_a_software_build(self):
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            (assets / "howff-gui-build.json").write_text(
                json.dumps({"backend": "sdl-gpu"})
            )
            with (
                patch.object(builder, "ASSETS", assets),
                patch.object(builder.gui, "artifacts") as verify,
                patch.object(builder.subprocess, "run") as compile_gui,
            ):
                builder.interface(False)
                verify.assert_called_once_with(assets)
                compile_gui.assert_not_called()

    def test_stale_gpu_inputs_fail_without_falling_back_to_software(self):
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            (assets / "howff-gui-build.json").write_text(
                json.dumps({"backend": "sdl-gpu"})
            )
            with (
                patch.object(builder, "ASSETS", assets),
                patch.object(
                    builder.gui, "artifacts", side_effect=ValueError("stale source")
                ),
                patch.object(builder.subprocess, "run") as compile_gui,
            ):
                with self.assertRaisesRegex(ValueError, "stale source"):
                    builder.interface(False)
                compile_gui.assert_not_called()

    def test_checks_still_validate_gui_sources(self):
        with patch.object(builder.subprocess, "run") as check:
            builder.interface(True)
            self.assertEqual(check.call_args.args[0][-1], "check")


if __name__ == "__main__":
    unittest.main()
