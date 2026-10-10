# Contribution checks

Run `sh scripts/quality.sh` before committing source changes. Builds require the prepared release assets and an existing Rust 1.92 toolchain with ARM64 musl, clang and llvm-ar. Keep compiler output under `/tmp/arkos-rust-build/` and distribution output under the ignored `dist/` directory.

Device behavior belongs to the native Rust runtime. Preserve manual entry points, owned session cleanup and original firmware, drivers, configurations and saves. Do not add boot services or install-time gameplay. The GUI uses mirui 0.48.0 declarative Flex compositions on the handheld framebuffer, cinnabar/ivory/ink colors and square corners. Text wraps or truncates through paragraph layout; focus follows computed target bounds. Keep the display Model registered once per App, static subtree caches separate from dynamic bindings, and focus damage limited to changed border strips.

Public documentation describes current behavior and measured compatibility. Renderer snapshots illustrate the interface; metadata checks and synthetic captures do not establish game-specific multiplayer acceptance. Code and comments use English. Markdown paragraphs use one physical line each.

README and release notes cover the actual product and its installation/play workflow. Keep conversation constraints and rejected proposals in agent instructions. Prefer concrete requirements and compatibility results over hypothetical contrasts or absence/guarantee disclaimers. Use a concise introduction, useful badges, interface previews and a short quick start.

Product README and changelog entries describe player actions and observable results. Routine implementation choices such as state models, DSL syntax, Flex layout, reactive bindings, framework versions and render caches belong in architecture or build documentation; do not present them as product features.
