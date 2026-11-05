use crate::model::{Page, Phase, View};

pub(super) fn host_action_enabled(view: &View) -> bool {
    view.host_action_enabled()
}

pub(super) fn page_heading(view: &View) -> &str {
    match view.page {
        Page::Lobby => "附近正在玩的伙伴",
        Page::Candidates => "选择本机的对应游戏",
        Page::Mode => "选择连接方式",
        Page::Choice if view.network_only => "和好友建立局域网",
        Page::Choice => "和好友一起，开始游戏",
        Page::Host => {
            if view.room_name.is_empty() {
                "正在创建房间"
            } else {
                &view.room_name
            }
        }
        Page::Rooms => "选择好友的房间",
        Page::Games => &view.title,
        Page::Joined => "加入房间",
        Page::Leaving => "退出联机",
        Page::Unavailable => "附近联机",
    }
}

pub(super) fn loading(view: &View) -> bool {
    if view.page == Page::Games && view.library.load == nearby_menu::LibraryLoad::Loading {
        return true;
    }
    if view.network_only && view.connected && view.phase == Phase::NetworkConnected {
        return view.busy;
    }
    if matches!(
        phase(view),
        Phase::SelectingGame
            | Phase::ApprovalPending
            | Phase::Closed
            | Phase::Failed
            | Phase::RestoreFailed
            | Phase::RoomActive
    ) {
        return false;
    }
    view.busy
        || view.refreshing
        || (matches!(view.page, Page::Rooms | Page::Lobby) && view.rooms.is_empty())
        || (view.page == Page::Host && !view.ready)
        || matches!(view.page, Page::Joined | Page::Leaving)
}

pub(super) fn phase(view: &View) -> Phase {
    if view.phase != Phase::Idle {
        return view.phase;
    }
    match view.page {
        Page::Joined => {
            if view.ready {
                Phase::WaitingStart
            } else {
                Phase::Connecting
            }
        }
        Page::Host => {
            if view.ready {
                Phase::HostReady
            } else {
                Phase::WaitingPeer
            }
        }
        Page::Leaving => Phase::Leaving,
        _ => Phase::Idle,
    }
}

pub(super) fn headline(view: &View) -> &str {
    match phase(view) {
        Phase::ResolvingGame => "正在查找本机游戏",
        Phase::SelectingGame => "选择对应游戏",
        Phase::NetworkConfirming => "正在确认连接",
        Phase::NetworkConnected => "组网已完成",
        Phase::NetworkReconnecting => "正在等待连接恢复",
        Phase::ApprovalPendingClient => "等待房主确认",
        Phase::Creating => "正在创建房间",
        Phase::WaitingPeer => "等待好友加入",
        Phase::PeerPreparing => "好友正在准备",
        Phase::ApprovalPending => "好友加入",
        Phase::HostReady => "可以开始游戏",
        Phase::Connecting => "正在连接房间",
        Phase::Checking => "正在核对游戏",
        Phase::Notifying => "正在确认准备完成",
        Phase::WaitingStart => "已就绪，等待房主开局",
        Phase::Starting => "正在启动游戏",
        Phase::Leaving => "正在退出联机",
        Phase::Closed => "本次联机已结束",
        Phase::Failed => "未能完成本次连接",
        Phase::RestoreFailed => "网络尚未恢复",
        Phase::RoomActive => "这台掌机已有房间",
        Phase::Idle => "请选择创建或加入",
    }
}

pub(super) fn next_step(view: &View) -> &str {
    if view.network_only {
        return match phase(view) {
            Phase::ResolvingGame | Phase::SelectingGame => "正在查找对应游戏",
            Phase::NetworkConnected if view.return_to_launcher => "返回收藏后，连接继续保持",
            Phase::NetworkConnected => "返回系统后，连接继续保持",
            Phase::NetworkConfirming => "正在确认双方的局域网连接",
            Phase::NetworkReconnecting => "持续失联后会恢复原 Wi-Fi",
            Phase::ApprovalPendingClient => "请房主按 A 确认加入",
            Phase::Connecting => "正在连接房间并获取地址",
            Phase::Leaving => "正在断开连接并恢复 Wi-Fi",
            Phase::Creating => "房间准备好后，好友才能加入",
            Phase::WaitingPeer => "请好友选择这个房间加入",
            Phase::ApprovalPending => "按 A 确认，B 关闭房间",
            Phase::Closed | Phase::Failed => "按 B 返回，重新创建或选择房间",
            Phase::RestoreFailed => "按 A 重试，完成后再返回",
            Phase::RoomActive => "按 A 关闭上一场后继续",
            Phase::PeerPreparing
            | Phase::HostReady
            | Phase::Checking
            | Phase::Notifying
            | Phase::WaitingStart
            | Phase::Starting => "正在确认双方的局域网连接",
            Phase::Idle if matches!(view.page, Page::Rooms | Page::Lobby) => {
                "选择房间，与附近掌机组网"
            }
            Phase::Idle => "方向键选择，按 A 确认",
        };
    }
    match phase(view) {
        Phase::ResolvingGame => "核对本地游戏内容与房间使用的核心",
        Phase::SelectingGame => "选择匹配的游戏后继续加入",
        Phase::NetworkConnected
        | Phase::NetworkConfirming
        | Phase::NetworkReconnecting
        | Phase::ApprovalPendingClient => "正在确认连接",
        Phase::Creating => "房间准备好后，好友才能加入",
        Phase::WaitingPeer => "请好友选择这个房间加入",
        Phase::PeerPreparing => "准备完成后，可以开始游戏",
        Phase::ApprovalPending => "按 A 确认，B 关闭房间",
        Phase::HostReady => "按 A 开始，双方一起进入游戏",
        Phase::Connecting => "连接完成后，会核对两台的游戏",
        Phase::Checking => "正在确认两台的游戏一致",
        Phase::Notifying => "房主确认就绪后，才能开始游戏",
        Phase::WaitingStart => "请房主按 A 开始游戏",
        Phase::Starting => "正在准备双方的联机画面",
        Phase::Leaving => "关闭本次连接，恢复原网络",
        Phase::Closed | Phase::Failed => "按 B 返回，重新创建或选择房间",
        Phase::RestoreFailed => "按 A 重试，完成后再返回",
        Phase::RoomActive => "按 A 关闭上一场后继续",
        Phase::Idle => "方向键选择，按 A 确认",
    }
}

pub(super) fn button_text(view: &View) -> &str {
    if view.network_only && view.connected && view.phase == Phase::NetworkConnected {
        return if view.return_to_launcher {
            "保持连接，返回收藏"
        } else {
            "保持连接，返回系统"
        };
    }
    match phase(view) {
        Phase::HostReady => {
            if view.busy {
                "正在开局"
            } else {
                "开始游戏"
            }
        }
        Phase::Creating => "房间准备中",
        Phase::PeerPreparing => "等待好友准备",
        Phase::ApprovalPending => "确认加入",
        Phase::Starting => "正在开局",
        Phase::Closed | Phase::Failed => "按 B 返回",
        Phase::Leaving | Phase::RestoreFailed => "正在退出房间",
        _ => "等待好友加入",
    }
}

pub(super) fn peer_text(view: &View) -> &str {
    match phase(view) {
        Phase::HostReady | Phase::Starting => "好友已就绪",
        Phase::PeerPreparing => "正在核对",
        Phase::ApprovalPending => "等待确认",
        Phase::Closed | Phase::Failed | Phase::Leaving => "联机已结束",
        _ => "等待好友",
    }
}

pub(super) fn status_text(view: &View) -> &str {
    if !view.status.is_empty() || view.launcher.enabled || view.page == Page::Games {
        return &view.status;
    }
    match view.page {
        Page::Lobby => "选择房间，向下展开创建与游戏库",
        Page::Candidates => "只切换本次联机使用的游戏",
        Page::Mode => "请选择仅组网或选择游戏",
        Page::Choice if view.network_only => "创建房间，或加入附近掌机的房间",
        Page::Host if view.network_only => next_step(view),
        Page::Choice => "两台掌机选择同一款游戏，再创建或加入房间",
        Page::Host => "请好友选择同一款游戏，加入这个房间",
        Page::Rooms => "选择房间后确认加入，列表会自动刷新",
        Page::Joined => next_step(view),
        Page::Leaving => "正在结束本次联机...",
        Page::Unavailable => "这款游戏暂时不支持附近联机",
        Page::Games => "选择游戏后创建或加入房间",
    }
}

pub(super) fn hint_text(view: &View) -> &str {
    if view.launcher.enabled && !view.status.is_empty() {
        return &view.status;
    }
    if view.launcher.enabled && !view.library.keyboard {
        if view.library.kind == crate::model::LibraryKind::Search {
            return if view.launcher.key.is_empty() {
                "Y 键盘"
            } else {
                "X 游玩 · Y 收藏 · START 键盘"
            };
        }
        return if view.library.kind == crate::model::LibraryKind::Favorites {
            if view.launcher.key.is_empty() {
                "Y 排序"
            } else {
                "X 游玩 / 长按联机 · Y 排序"
            }
        } else if !view.launcher.key.is_empty() {
            "X 游玩 / 长按联机 · Y 收藏"
        } else {
            "Y 搜索"
        };
    }
    if view.page == Page::Games && !view.busy {
        return match view.library.kind {
            crate::model::LibraryKind::Platforms => "Y 搜索 · L/R 翻页",
            crate::model::LibraryKind::Favorites => "X 移除 · Y 排序",
            crate::model::LibraryKind::Search if view.library.keyboard => "Y 退格 · START 结果",
            crate::model::LibraryKind::Search => "X 收藏 · Y 键盘",
            _ => "X 收藏 · L/R 翻页",
        };
    }
    if view.page == Page::Lobby {
        return "下 / R 展开操作";
    }
    if view.page == Page::Mode && view.lobby {
        return "上 / L 展开房间";
    }
    if view.network_only && view.connected && view.phase == Phase::NetworkConnected {
        return "连接保持中";
    }
    if matches!(view.page, Page::Games | Page::Candidates) {
        return if view.busy {
            "正在核对游戏"
        } else {
            "L/R 翻页"
        };
    }
    match phase(view) {
        Phase::RestoreFailed => "按 A 重试退出",
        Phase::RoomActive => "A 关闭后继续",
        Phase::Leaving => "等待网络恢复",
        Phase::WaitingStart => "等待房主开局",
        Phase::HostReady => "A 开始游戏",
        Phase::ApprovalPending => "按 A 确认",
        Phase::Creating | Phase::WaitingPeer | Phase::PeerPreparing => "B 关闭房间",
        Phase::Connecting | Phase::Checking | Phase::Notifying => "B 取消加入",
        Phase::Starting => "正在进入游戏",
        Phase::Closed | Phase::Failed => "B 返回后重试",
        _ => {
            if matches!(view.page, Page::Rooms | Page::Lobby) {
                "自动搜索"
            } else {
                "方向键选择"
            }
        }
    }
}

pub(super) fn confirm_text(view: &View) -> &str {
    if view.launcher.enabled {
        if view.busy || view.launcher.navigation {
            return "";
        }
        if view.launcher.tab == 0
            && view
                .entries
                .get(view.selected)
                .is_some_and(|row| row.directory && row.icon == "library" && row.available)
        {
            return "游戏库";
        }
    }
    if view.launcher.enabled && !view.library.keyboard && !view.launcher.key.is_empty() {
        return "操作";
    }
    if view.page == Page::Games && view.library.keyboard && !view.busy {
        return "输入";
    }
    if view.page == Page::Candidates {
        return if view
            .entries
            .get(view.selected)
            .is_some_and(|row| row.available)
            && !view.busy
        {
            "使用此游戏"
        } else {
            ""
        };
    }
    if view.network_only && view.connected && view.phase == Phase::NetworkConnected && !view.busy {
        return if view.return_to_launcher {
            "返回收藏"
        } else {
            "返回系统"
        };
    }
    match phase(view) {
        Phase::RestoreFailed => "重试",
        Phase::RoomActive => "关闭",
        Phase::Closed | Phase::Failed => "返回",
        Phase::HostReady if !view.busy => "开始",
        Phase::ApprovalPending if !view.busy => "确认",
        Phase::Idle => match view.page {
            Page::Choice | Page::Mode => "确认",
            Page::Rooms | Page::Lobby
                if !view.busy
                    && view
                        .rooms
                        .get(view.selected)
                        .is_some_and(|room| room.available) =>
            {
                "加入"
            }
            Page::Games | Page::Candidates if !view.busy && !view.entries.is_empty() => "选择",
            _ => "",
        },
        _ => "",
    }
}
