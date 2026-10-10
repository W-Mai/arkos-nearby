# Changelog

## [Unreleased]

## [0.3.2] - 2026-10-10

- Keep long game names and connection messages within their panels.
- Align the selection border with the selected item during movement and resizing.

## [0.3.1] - 2026-10-10

- Use gpSP automatic cartridge protocol detection for GBA Netpacket sessions.

- Select FBNeo for compatible arcade games through isolated ROM loading and rollback-state checks during room preparation.

## [0.3.0] - 2026-10-10

- Faster full-state compression for the verified ARM64 RetroArch build.
- Bundled Nestopia core with complete input-state restoration and zeroed state padding.
- FBNeo selection from MAME for verified Final Fight content, restricted to matching 64-bit source and target builds.
- Super Bomberman 5 retains its supported local core.
- Explicit controller ports and a 600-frame state verification interval for multiplayer sessions.

## [0.2.0] - 2026-10-09

- Native Rust installer and nearby room runtime for the checked ArkOS4Clone / RTL8188EU profile.
- Graphical local game selection from Options and current-game entry before launch with X.
- Selected-room discovery, manual refresh, owner-confirmed first pairing and remembered device identities.
- Session preparation, matching local frontend/core/content identities and explicit host start.
- Session-owned game handoff and restoration of the original wireless driver and network.
- Chinese mirui interface with cinnabar, ivory and ink colors, square corners and bounded dirty-region rendering.
- GB/GBC and selected GBA link configuration using installed cores, with game-specific acceptance limits.
- Complete Chinese title bounds and separate subtitle spacing.

The release supports two-device rooms and temporary session saves. Arcade performance and handheld link acceptance remain game-specific; see [compatibility](docs/compatibility.md).
