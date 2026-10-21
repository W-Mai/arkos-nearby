# Changelog

## [Unreleased]

## [0.9.1] - 2026-10-21

- Improve animation performance in the game library, Favorites and multiplayer panels.

## [0.9.0] - 2026-10-20

- Add Nearby Launcher in Options, opening personal favorites with single-player launch and current-game multiplayer panels.
- Arrange landscape favorite cartridges in one or two rows, with game artwork, directional paging and name/recent sorting.
- Expand the game library beneath a miniature Favorites shelf and restore selection when returning.
- Launch single-player with a short X press and open multiplayer with a held X press, preserving the system's game settings and saves.
- Add illustrated game actions and settings, with character movement, themes and menu sounds.
- Improve library browsing and panel response while retaining game artwork during background updates.
- Keep cartridge rows and action hints stable during held-X feedback and game preparation.

## [0.8.1] - 2026-10-17

- Show cached game previews immediately and preload adjacent game artwork.

- Speed up game library browsing, searches and local game matching when joining rooms.

- Show matching local games when joining a room directly and replace stale library rows with current matches.

## [0.8.0] - 2026-10-17

- Preserve game library contents while returning to the previous page.
- Open the game library faster and keep platform counts visible during background updates.
- Add persistent favorites with horizontal cartridge cards, name/recent sorting, and cached game search by title, filename and pinyin.
- Show distinct console illustrations, larger game previews and clearer game titles in the library.
- Reduce frame time when expanding nearby rooms and creation controls on the home screen.

## [0.7.0] - 2026-10-17

- Keep FN + X exclusive to Nearby Multiplayer while preserving the home menu's standalone X action.
- Fix a black screen when opening nearby room discovery.
- Expand room discovery and creation controls in place, with a miniature preview of the collapsed area.
- Open Create Room / Join Room for the current game directly when holding X during launch.
- Show nearby game and network rooms on entry, with quick access to room creation and the game library.
- Find matching local games when joining a room and select the version to use for the session.
- Show library titles, platform icons, game counts and screenshots.
- Open Nearby Multiplayer from the system home with FN + X.

## [0.6.2] - 2026-10-16

- Start page entrance animations when the interface is ready to display.

## [0.6.1] - 2026-10-16

- Ensure the audio probe plays its confirmation cue.

## [0.6.0] - 2026-10-16

- Add 8-bit menu sounds for navigation, room discovery, readiness and connection results.
- Reduce animated SVG size and browser rendering overhead.
- Add animated SVG and H.264 MP4 export commands for the dual-handheld demo.

## [0.5.1] - 2026-10-16

- Add side markers and bottom shadows to room and game rows, with sparse lists aligned at the top.
- Keep selection stable while room lists refresh and refine button and character feedback.
- Improve connection panels and show join hints for available rooms.
- Correct network-only room guidance and keep hint text clear of panel shadows.

## [0.5.0] - 2026-10-16

- Add Forest, Sunset and Night themes with saved Y-button selection.
- Add a campsite emblem, mountain scenery, side ornaments and raised buttons.
- Use crisp pixel lettering throughout the interface.
- Animate page entry, focused buttons, character movement and room readiness.
- Improve menu rendering and controller response with GPU acceleration.
- Keep preparing-room buttons readable and jumping characters inside their panels.
- Align device names and addresses in network-only rooms.

## [0.4.0] - 2026-10-11

- Add Network Only and Choose Game entries in Options.
- Keep the nearby network connected while returning to the system and opening LAN games from Ports.
- Show connected device addresses and restore Wi-Fi when leaving the network.
- Clear peer addresses after disconnection.

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
