use crate::Result;
pub use crate::platform::system::Tool;
pub(crate) use crate::platform::system::recover_pending;
use howff_menu::{MenuAction, MenuItem};

impl Tool {
    pub(crate) const ALL: [Self; 2] = [Self::Files, Self::SystemInfo];

    pub(crate) fn action(self) -> MenuAction {
        match self {
            Self::SystemInfo => MenuAction::SystemInfo,
            Self::Files => MenuAction::FileManager,
        }
    }

    pub(crate) fn from_action(action: MenuAction) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.action() == action)
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::SystemInfo => "系统信息",
            Self::Files => "文件管理",
        }
    }

    fn available(self) -> std::result::Result<(), String> {
        crate::platform::system::check_tool_runtime()?;
        crate::session::require_idle().map_err(|_| "请先结束房间并完成网络恢复".to_string())?;
        crate::platform::system::check_tool(self)
    }

    pub(super) fn prepare(self) -> Result<crate::platform::system::Prepared> {
        self.available()?;
        recover_pending()?;
        crate::platform::system::prepare_tool(self)
    }
}

#[derive(Default)]
pub(super) struct Tools {
    items: Vec<MenuItem>,
}

impl Tools {
    pub(super) fn refresh(&mut self) {
        self.items = [
            MenuItem::available(MenuAction::OpenWifi, "Wi-Fi 设置"),
            MenuItem::available(MenuAction::NetworkInfo, "网络信息"),
            MenuItem::available(MenuAction::ShowStorage, "存储空间"),
        ]
        .into_iter()
        .chain(Tool::ALL.into_iter().map(|tool| match tool.available() {
            Ok(()) => MenuItem::available(tool.action(), tool.name()),
            Err(reason) => MenuItem::unavailable(tool.action(), tool.name(), reason),
        }))
        .chain([MenuItem::available(MenuAction::Back, "返回")])
        .collect();
    }

    pub(super) fn items(&self) -> Vec<MenuItem> {
        self.items.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_preserve_selection_and_return_through_the_existing_menu_hierarchy() {
        use super::super::{Controller, Overlay, controls};
        for back in ["back", "confirm"] {
            let mut controller = Controller {
                browsers: [
                    crate::catalog::Browser::new().unwrap(),
                    crate::catalog::Browser::new().unwrap(),
                ],
                tab: 1,
                overlay: Some(Overlay::Tools),
                selection: 0,
                entry: None,
                controls: controls::Controls::default(),
                tools: Tools {
                    items: Tool::ALL
                        .into_iter()
                        .map(|tool| MenuItem::available(tool.action(), tool.name()))
                        .chain([MenuItem::available(MenuAction::Back, "返回")])
                        .collect(),
                },
                details: Default::default(),
                wifi: Default::default(),
                wifi_return: MenuAction::OpenWifi,
                configuration: Default::default(),
                saves: Default::default(),
                saves_from_configuration: false,
                management: Default::default(),
                detail_expanded: false,
                detail_scroll: 0,
                changed_tab: false,
                status: String::new(),
            };
            let browser = controller.browser().view();
            for (index, tool) in Tool::ALL.into_iter().enumerate() {
                controller.selection = index;
                assert!(
                    matches!(controller.input("confirm").unwrap(), Some(super::super::Intent::Tool(value)) if value == tool)
                );
                assert!(controller.overlay == Some(Overlay::Tools));
                assert_eq!(controller.selection, index);
                assert_eq!(controller.browser().view(), browser);
            }
            controller.tools.items[0] = MenuItem::unavailable(
                MenuAction::OpenWifi,
                "Wi-Fi 设置",
                "请先结束房间并完成网络恢复",
            );
            controller.selection = 0;
            assert!(controller.input("confirm").unwrap().is_none());
            assert!(controller.overlay == Some(Overlay::Tools));
            assert!(!controller.status.is_empty());
            if back == "confirm" {
                controller.selection = controller.options().len() - 1;
            }
            assert!(controller.input(back).unwrap().is_none());
            assert!(controller.overlay == Some(Overlay::Controls));
            assert_eq!(
                controller.options()[controller.selection].action,
                MenuAction::ShowTools
            );
            assert_eq!(controller.tab, 1);
            controller.input("back").unwrap();
            assert!(controller.overlay == Some(Overlay::System));
            assert_eq!(
                controller.options()[controller.selection].action,
                MenuAction::ShowControls
            );
            controller.input("back").unwrap();
            assert!(controller.overlay.is_none());
            assert_eq!(controller.browser().view(), browser);
        }
    }
}
