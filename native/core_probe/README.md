# Arcade load probe

`core-probe64 CORE ROM SYSTEM_DIR PRIVATE_SAVE_DIR OPTIONS_FILE` loads a local FinalBurn Neo archive and checks state serialization and restoration using the rollback context. The caller provides a private empty save directory, a core-options snapshot and a sandbox with read-only access to the original files.

The program returns a single JSON object after loading, state checks and cleanup succeed. `schema` is `1`, `loaded` is `true`, `state_bytes` is positive, and `timings_ms` contains `open`, `init`, `load`, `state`, `cleanup` and `total`. Failure returns a nonzero exit code with bounded diagnostics on stderr. A successful result describes archive loading and state support; gameplay validation measures progression, controls and sustained performance separately.

Build with `python3 scripts/build_core_probe.py build`. `check` validates the ARM64 source offline. The libretro ABI declarations are a subset of the v1 header distributed with RetroArch 1.22.2; its license is retained in `ABI-LICENSE.txt`.
