#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pane {
    Overlay,
    Wifi,
    WifiKeyboard,
    Management,
    NameEditor,
    Saves,
    Configuration,
    Details,
    Levels,
}
