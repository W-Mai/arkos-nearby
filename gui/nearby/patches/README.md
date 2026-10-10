# mirui 0.48.0 patch

`mirui-0.48.0.patch` provides entity-local Text invalidation and App layout lifecycle methods used by the responsive room interface.

- Text changes invalidate the owning widget and preserve unrelated text caches. Optional text-path changes retain the same ownership rule.
- Reflow invalidates widgets whose computed bounds change, so moved siblings repaint both their previous and current positions.
- `App::prepare_layout()` resolves reactive updates and layout before geometry is read, without painting a frame.
- `App::invalidate_rect()` marks explicit damage for the animated focus border.
- `App::clear_root()` releases the previous page through App ownership.

The GUI build script verifies the cached registry crate against `Cargo.lock`, applies the patch in `/tmp`, and builds a staged GUI package with a derived lock. Build records include the base crate checksum, patch checksum and derived lock checksum. Run checks and builds through the script:

```sh
python3 scripts/build_nearby_gui.py check
python3 scripts/build_nearby_gui.py build
```

To inspect or apply the patch in a mirui v0.48.0 checkout, use its absolute path:

```sh
git apply --check /path/to/arkos-nearby/gui/nearby/patches/mirui-0.48.0.patch
git apply /path/to/arkos-nearby/gui/nearby/patches/mirui-0.48.0.patch
```

The patch includes framework regression tests for isolated text updates, reactive layout preparation, exact damage, and page teardown. Text growth and shrinkage move a sibling widget; incremental rendering after layout preparation is compared pixel by pixel with a full frame. Application checks cover responsive geometry, incremental rendering, cache retention, focus movement and page replacement.
