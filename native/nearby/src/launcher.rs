pub mod recipes;

use crate::catalog::{Browser, Entry};
use crate::flow::FlowExit;
use crate::menu::MenuSession;
use crate::{Result, require};
use serde_json::{Value, json};
use std::path::Path;
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub enum Intent {
    Play(Entry),
    RestorePlay(Entry),
    Multiplayer(Entry),
    Discovery,
    Network,
    Exit,
    Power,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    Game,
    System,
    Exit,
    Power,
    Network,
}

pub struct Controller {
    browsers: [Browser; 2],
    tab: usize,
    overlay: Option<Overlay>,
    selection: usize,
    entry: Option<Entry>,
    pub changed_tab: bool,
    pub status: String,
}

impl Controller {
    pub fn new() -> Result<Self> {
        let mut result = Self {
            browsers: [Browser::launcher(true)?, Browser::launcher(false)?],
            tab: 0,
            overlay: None,
            selection: 0,
            entry: None,
            changed_tab: false,
            status: String::new(),
        };
        if let Ok(saved) =
            crate::system::read::<Value>(&Path::new(crate::context::INSTALL).join("launcher.json"))
        {
            if let Some(key) = saved["last_favorite"].as_str() {
                result.browsers[0].select_key(key);
            }
        }
        Ok(result)
    }
    pub fn shelf_layout(&mut self, grid: crate::catalog::shelf::Grid) {
        self.browsers[0].shelf_layout(grid);
    }
    pub fn browser(&mut self) -> &mut Browser {
        &mut self.browsers[self.tab.min(1)]
    }

    pub(crate) fn indexed(&mut self, index: std::sync::Arc<crate::catalog::Index>) {
        for browser in &mut self.browsers {
            browser.accept_index(index.clone());
        }
    }

    fn sync_personal(&mut self) {
        for browser in &mut self.browsers {
            browser.refresh_personal();
        }
    }

    fn reopen(&mut self) {
        for browser in &mut self.browsers {
            browser.reopen();
        }
    }
    fn game(&self) -> Option<Entry> {
        self.browsers[self.tab.min(1)].game()
    }
    fn options(&self) -> Vec<(&'static str, bool)> {
        match self.overlay {
            Some(Overlay::Game) => vec![
                ("开始游戏", true),
                ("附近联机", self.entry.as_ref().is_some_and(Entry::netplay)),
                (
                    if self.browsers[self.tab].is_favorite() {
                        "移出收藏"
                    } else {
                        "收藏游戏"
                    },
                    true,
                ),
                ("返回", true),
            ],
            Some(Overlay::System) => vec![
                ("切换主题", true),
                ("音效开关", true),
                ("管理直连网络", Self::retained()),
                ("返回原系统", true),
                ("关机", true),
            ],
            Some(Overlay::Exit) => vec![("返回", true), ("退出并恢复 Wi-Fi", true)],
            Some(Overlay::Power) => vec![("返回", true), ("确认关机", true)],
            Some(Overlay::Network) => vec![
                ("保留直连，开始游戏", true),
                ("恢复 Wi-Fi 后开始", true),
                ("返回", true),
            ],
            None => vec![],
        }
    }
    pub fn retained() -> bool {
        crate::session::state()
            .is_ok_and(|s| s.purpose == crate::purpose::SessionPurpose::NetworkOnly && !s.closed())
    }
    fn show(&mut self, overlay: Overlay) {
        self.overlay = Some(overlay);
        self.selection = 0;
        self.entry = self.game();
    }
    pub fn view(&mut self) -> Value {
        let mut view = self.browser().view();
        let shelf = self.browsers[0].view();
        let catalog = self.browsers[1].view();
        let game = self.game();
        let options = self.options();
        let title = match self.overlay {
            Some(Overlay::Game) => self.entry.as_ref().map(|e| e.name()).unwrap_or("游戏操作"),
            Some(Overlay::System) => "启动器设置",
            Some(Overlay::Exit) => "返回原系统？",
            Some(Overlay::Power) => "关闭掌机？",
            Some(Overlay::Network) => "当前保留着直连网络",
            None => "",
        };
        view["launcher"] = json!({"enabled":true,"tab":self.tab,"navigation":false,"key":game.as_ref().map(Entry::key).unwrap_or_default(),"netplay":game.as_ref().is_some_and(Entry::netplay),"overlay":title,"overlay_kind":match self.overlay {Some(Overlay::Game)=>"game",Some(Overlay::System)=>"system",Some(Overlay::Exit)=>"exit",Some(Overlay::Power)=>"power",Some(Overlay::Network)=>"network",None=>""},"items":options.iter().map(|(name,enabled)|json!({"name":name,"enabled":enabled})).collect::<Vec<_>>(),"selected":self.selection,"retained":Self::retained()});
        view["launcher_shelf"] = shelf;
        view["launcher_catalog"] = catalog;
        if self.tab == 0 {
            view["core"] = "本机游戏".into();
            view["title"] = "个人收藏".into();
        }
        if Self::retained() {
            view["core"] =
                format!("{} · 直连已保留", view["core"].as_str().unwrap_or_default()).into();
        }
        if !self.status.is_empty() {
            view["status"] = self.status.clone().into();
        }
        view["back_label"] = if self.tab == 0 || self.browser().root() {
            "返回"
        } else {
            "上级"
        }
        .into();
        view
    }
    fn tab(&mut self, value: usize) -> Option<Intent> {
        self.tab = value;
        self.changed_tab = true;
        self.browser().resume();
        None
    }
    pub fn gesture(&mut self, action: &str, key: &str) -> Result<Option<Intent>> {
        if self.overlay.is_some() {
            return Ok(None);
        }
        let Some(entry) = self.game().filter(|e| e.key() == key) else {
            return Ok(None);
        };
        self.remember(&entry)?;
        match action {
            "launch" if Self::retained() => {
                self.show(Overlay::Network);
                Ok(None)
            }
            "launch" => Ok(Some(Intent::Play(entry))),
            "multiplayer" if entry.netplay() => Ok(Some(Intent::Multiplayer(entry))),
            "multiplayer" => {
                self.status = "此游戏的启动方式暂未接入附近联机".into();
                Ok(None)
            }
            _ => Ok(None),
        }
    }
    fn remember(&self, entry: &Entry) -> Result<()> {
        if self.tab == 0 {
            crate::system::write(
                &Path::new(crate::context::INSTALL).join("launcher.json"),
                &json!({"schema":1,"last_favorite":entry.key()}),
            )?;
        }
        Ok(())
    }
    pub fn input(&mut self, action: &str) -> Result<Option<Intent>> {
        self.status.clear();
        if let Some(overlay) = self.overlay {
            let options = self.options();
            if action == "back" {
                self.overlay = None;
                return Ok(None);
            }
            if matches!(action, "up" | "down" | "left" | "right") {
                let delta = match action {
                    "up" => -2,
                    "down" => 2,
                    "left" => -1,
                    _ => 1,
                };
                self.selection =
                    (self.selection as isize + delta).clamp(0, options.len() as isize - 1) as usize;
                return Ok(None);
            }
            if action != "confirm" || !options[self.selection].1 {
                return Ok(None);
            }
            let selected = self.selection;
            self.overlay = None;
            return match (overlay, selected) {
                (Overlay::Game, 0) => {
                    let key = self.entry.as_ref().map(Entry::key).unwrap_or_default();
                    self.gesture("launch", &key)
                }
                (Overlay::Game, 1) => {
                    let key = self.entry.as_ref().map(Entry::key).unwrap_or_default();
                    self.gesture("multiplayer", &key)
                }
                (Overlay::Game, 2) => {
                    self.browser().dispatch("refresh")?;
                    self.sync_personal();
                    Ok(None)
                }
                (Overlay::System, 2) => Ok(Some(Intent::Network)),
                (Overlay::System, 3) => {
                    self.show(Overlay::Exit);
                    Ok(None)
                }
                (Overlay::System, 4) => {
                    self.show(Overlay::Power);
                    Ok(None)
                }
                (Overlay::Exit, 1) => Ok(Some(Intent::Exit)),
                (Overlay::Power, 1) => Ok(Some(Intent::Power)),
                (Overlay::Network, 0 | 1) => Ok(self.entry.clone().map(if selected == 1 {
                    Intent::RestorePlay
                } else {
                    Intent::Play
                })),
                _ => Ok(None),
            };
        }
        if action == "launcher_home" {
            return Ok(Some(Intent::Discovery));
        }
        if self.browser().editing() {
            self.browser().dispatch(action)?;
            return Ok(None);
        }
        if action == "start" && !self.browser().searching() {
            self.show(Overlay::System);
            return Ok(None);
        }
        if action == "back" && self.tab == 0 {
            return Ok(None);
        }
        if action == "back" && self.browser().root() {
            return Ok(self.tab(0));
        }
        if action == "auxiliary" && self.tab == 1 && self.game().is_some() {
            self.browser().dispatch("refresh")?;
            self.sync_personal();
            return Ok(None);
        }
        if action == "confirm" {
            if self.game().is_some() {
                self.show(Overlay::Game);
                return Ok(None);
            }
            if self.tab == 0
                && self
                    .browser()
                    .current()
                    .is_some_and(|entry| entry.library_link())
            {
                return Ok(self.tab(1));
            }
        }
        self.browser().dispatch(action)?;
        Ok(None)
    }
    pub fn restore_network(&mut self) -> Result<()> {
        if !Path::new(crate::session::BASE)
            .join("session.json")
            .exists()
        {
            return Ok(());
        }
        let s = crate::session::state()?;
        if !s.closed() || !s.network_restored {
            crate::session::recover(&s.id, "launcher_exit")?;
        }
        require(
            crate::session::state().is_ok_and(|s| s.closed() && s.network_restored),
            "Network recovery is unfinished",
        )
    }
}

pub fn run() -> Result<i32> {
    crate::session::identity(false)?;
    let mut controller = Controller::new()?;
    let mut display = None;
    loop {
        if display.is_none() {
            display = Some(MenuSession::open_launcher()?);
        }
        let intent = display.as_mut().unwrap().choose_launcher(&mut controller)?;
        let result = (|| -> Result<bool> {
            match intent {
                Intent::Play(ref entry) | Intent::RestorePlay(ref entry) => {
                    display
                        .as_mut()
                        .unwrap()
                        .launcher_busy(&mut controller, "正在准备游戏")?;
                    if matches!(intent, Intent::RestorePlay(_)) {
                        controller.restore_network()?;
                    }
                    let prepared = recipes::prepare(entry.path())?;
                    drop(display.take());
                    single_player(prepared)?;
                }
                Intent::Multiplayer(entry) => {
                    let menu = display.as_mut().unwrap();
                    if Controller::retained() {
                        menu.run_network()?;
                        if Controller::retained() {
                            return Ok(false);
                        }
                    }
                    menu.launcher_busy(&mut controller, "正在准备联机")?;
                    let context = entry.prepare()?;
                    menu.run_with_fallback(
                        Some(context.clone()),
                        Some(crate::session::NativeBackend::shared(context)),
                        false,
                    )?;
                }
                Intent::Discovery => {
                    let menu = display.as_mut().unwrap();
                    while let Some(mode) = menu.choose_mode()? {
                        match mode {
                            FlowExit::SelectGame => {
                                controller.tab(1);
                                break;
                            }
                            FlowExit::NetworkOnly => {
                                if menu.run_network()? {
                                    break;
                                }
                            }
                            FlowExit::KeepNetwork => break,
                            FlowExit::CurrentGame => {}
                        }
                    }
                }
                Intent::Network => {
                    display.as_mut().unwrap().run_network()?;
                }
                Intent::Exit | Intent::Power => {
                    display
                        .as_mut()
                        .unwrap()
                        .launcher_busy(&mut controller, "正在退出并恢复网络")?;
                    controller.restore_network()?;
                    drop(display.take());
                    if matches!(intent, Intent::Power) {
                        crate::system::command("systemctl", &["poweroff"], 10, true)?;
                    }
                    return Ok(true);
                }
            }
            if crate::session::state()
                .is_ok_and(|s| s.purpose == crate::purpose::SessionPurpose::Game && !s.closed())
            {
                drop(display.take());
                wait_game()?;
            }
            Ok(false)
        })();
        match result {
            Ok(true) => return Ok(0),
            Ok(false) => {}
            Err(error) => {
                controller.status = format!("操作未完成：{error}");
                crate::menu::log(json!({"launcher_error":error.to_string()}))?;
            }
        }
        controller.reopen();
    }
}

fn wait_game() -> Result<()> {
    while let Ok(s) = crate::session::state() {
        if s.purpose != crate::purpose::SessionPurpose::Game || s.closed() {
            break;
        }
        if s.phase == "restore_failed" {
            return Err("Network restoration failed; reopen Nearby to retry".into());
        }
        thread::sleep(Duration::from_millis(250));
    }
    if let Ok(s) = crate::session::state() {
        require(
            s.network_restored || s.purpose == crate::purpose::SessionPurpose::NetworkOnly,
            "Network restoration failed",
        )?;
    }
    Ok(())
}

fn single_player(launch: recipes::Launch) -> Result<()> {
    use std::process::{Command, Stdio};
    crate::menu::log(json!({"single_player":launch.path,"launch_command":launch.command}))?;
    let account = crate::session::cmd(&["getent", "passwd", "ark"])?;
    let uid = account
        .split(':')
        .nth(2)
        .ok_or("Missing game account")?
        .parse::<u32>()?;
    let unit = format!("arkos-nearby-solo-{}", crate::system::random_id()?);
    struct GameUnit(String);
    impl Drop for GameUnit {
        fn drop(&mut self) {
            let _ = crate::session::cmd(&["systemctl", "stop", &self.0]);
        }
    }
    let _unit = GameUnit(unit.clone());
    let log =
        std::fs::File::create(Path::new(crate::context::INSTALL).join("logs/launcher-game.log"))?;
    let mut child = crate::menu::OwnedChild(
        Command::new("systemd-run")
            .args([
                "--collect",
                "--wait",
                &format!("--unit={unit}"),
                "--property=User=ark",
                "--property=WorkingDirectory=/home/ark",
                "--property=StandardInput=tty",
                "--property=TTYPath=/dev/tty1",
                "--property=KillMode=control-group",
                "--property=TimeoutStopSec=10",
                "--property=BindsTo=arkos-nearby-launcher.service",
                "--property=After=arkos-nearby-launcher.service",
                "--property=ExecStopPost=/opt/arkos-nearby/arkos-nearby launcher-game-end",
                "--setenv=HOME=/home/ark",
                &format!("--setenv=XDG_RUNTIME_DIR=/run/user/{uid}"),
                "--setenv=TERM=linux",
                "/bin/bash",
                "-c",
                &launch.command,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?,
    );
    let mut recorded = false;
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if !recorded && running_frontend(&unit, &launch.path) {
            crate::catalog::library::record_path(&launch.path);
            recorded = true;
        }
        thread::sleep(Duration::from_millis(150));
    };
    require(status.success(), "Game process exited with an error")
}

fn running_frontend(unit: &str, game: &Path) -> bool {
    std::fs::read_dir("/proc").is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let path = entry.path();
            std::fs::read_to_string(path.join("cgroup")).is_ok_and(|text| {
                text.lines()
                    .any(|line| line.ends_with(&format!("/{unit}.service")))
            }) && std::fs::read_link(path.join("exe")).is_ok_and(|exe| {
                !matches!(
                    exe.file_name().and_then(|name| name.to_str()),
                    Some("bash" | "sh" | "dash" | "sudo" | "env" | "nice" | "ffplay")
                ) && (std::fs::read(path.join("cmdline")).is_ok_and(|args| {
                    args.split(|byte| *byte == 0)
                        .any(|arg| arg == game.as_os_str().as_encoded_bytes())
                }) || game.extension().is_some_and(|ext| ext == "sh")
                    && game.parent().is_some_and(|parent| exe.starts_with(parent)))
            })
        })
    })
}
