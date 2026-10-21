# Contribution checks

Menu typography uses Fusion Pixel with native 12px and 24px assets. Select the matching font face through the shared style helper, including header titles; preserve the catalog charset and combined font licenses when rebuilding assets.

Run `sh scripts/quality.sh` before committing source changes. Builds require the prepared release assets and an existing Rust 1.92 toolchain with ARM64 musl, clang and llvm-ar. Keep compiler output under `/tmp/arkos-rust-build/` and distribution output under the ignored `dist/` directory.

Device behavior belongs to the native Rust runtime. Preserve manual entry points, owned session cleanup and original firmware, drivers, configurations and saves. Do not add boot services or install-time gameplay. The GUI uses mirui 0.51.0 declarative Flex compositions on the handheld framebuffer, cinnabar/ivory/ink colors and square corners. Text wraps or truncates through paragraph layout; FollowBounds follows the selected widget and PageView owns page replacement. Keep the display Model registered once per App, static subtree caches separate from dynamic bindings, and focus damage limited to changed border strips.

`purpose.rs` defines game and network-only session intents. `network.rs` owns authenticated network membership and peer leases. Options offers Network Only and Choose Game; retaining a network closes the interface while membership continues. Recovery owns only the active session's network resources and preserves independently launched Ports processes. Keep ended-session addresses and peer lists empty.

Keep one MenuSession for the entire Options interaction. menu/display.rs owns the GUI and mapper processes; changing modes, browsing games and returning from a room must not restart them. Background replies belong to their originating screen, and queued input must not leak across screen handoffs. The GUI render modules separate state, copy, style, fonts, shared widgets, pages and composition; model.rs remains the serialized display protocol.

Public documentation describes current behavior and measured compatibility. Renderer snapshots illustrate the interface; metadata checks and synthetic captures do not establish game-specific multiplayer acceptance. Code and comments use English. Markdown paragraphs use one physical line each.

The released GPU interface uses the same scene composition through Surface and RendererFactory, with SDL2 2.32.10 linked statically. scripts/build_nearby_gpu.py supports a separate dynamic-library build. Keep shared SDL libraries scoped to their output directory, retain persistent GLES frame targets, and record hardware acceptance separately from compilation. Framebuffer builds remain available through scripts/build_nearby_gui.py.

README and release notes cover the actual product and its installation/play workflow. Keep conversation constraints and rejected proposals in agent instructions. Prefer concrete requirements and compatibility results over hypothetical contrasts or absence/guarantee disclaimers. Use a concise introduction, useful badges, interface previews and a short quick start.

Product README and changelog entries describe player actions and observable results. Routine implementation choices such as state models, DSL syntax, Flex layout, reactive bindings, framework versions and render caches belong in architecture or build documentation; do not present them as product features.
