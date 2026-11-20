#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pane {
    Overlay,
    Cartridge(u64),
    Wifi,
    WifiKeyboard,
    Management,
    NameEditor,
    Saves,
    Configuration,
    Details,
    Levels,
}
