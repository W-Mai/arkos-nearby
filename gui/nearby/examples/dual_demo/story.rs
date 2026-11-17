use super::model::{NetworkPeer, Page, Phase, RoomRow, View};
use super::scene::{Frame, Pose};
use super::theme::Palette;

pub const FPS: u32 = 25;
pub const SECONDS: f32 = 20.0;
pub const FRAMES: u32 = FPS * SECONDS as u32;

pub struct Screen {
    pub view: View,
    pub palette: Palette,
    pub press: u8,
}

pub struct Shot {
    pub screens: [Screen; 2],
    pub frame: Frame,
}

fn base() -> View {
    View {
        title: "魂斗罗".into(),
        core: "NES / FC".into(),
        ..View::default()
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn envelope(t: f32, begin: f32, end: f32) -> f32 {
    smooth((t - begin) / 0.75) * (1.0 - smooth((t - end) / 0.75))
}

fn pulse(t: f32, events: &[f32]) -> f32 {
    events
        .iter()
        .map(|event| {
            let age = t - event;
            if (0.0..0.48).contains(&age) {
                (1.0 - age / 0.48).powi(2)
            } else {
                0.0
            }
        })
        .fold(0.0, f32::max)
}

pub fn at(t: f32) -> Shot {
    let t = t.rem_euclid(SECONDS);
    let mut left = base();
    let mut right = base();
    right.selected = 1;
    let stage = match t {
        t if t < 2.0 => 0,
        t if t < 4.8 => 1,
        t if t < 7.6 => 2,
        t if t < 10.0 => 3,
        t if t < 12.6 => 4,
        t if t < 15.5 => 5,
        t if t < 18.0 => 6,
        _ => 0,
    };
    if (2.6..12.6).contains(&t) {
        left.page = Page::Host;
        left.phase = if t < 3.3 {
            Phase::Creating
        } else {
            Phase::WaitingPeer
        };
        left.busy = t < 3.3;
        if t >= 3.3 {
            left.room_name = "房间 A216".into();
        }
    }
    if (5.3..7.0).contains(&t) {
        right.page = Page::Rooms;
        right.selected = 0;
        right.refreshing = t < 6.5;
        if t >= 5.9 {
            right.rooms.push(RoomRow {
                key: "demo-room".into(),
                name: "房间 A216".into(),
                detail: "同一游戏，可以加入".into(),
                signal: 4,
                available: true,
            });
            right.status = "已发现附近房间".into();
        }
    }
    if (7.0..12.6).contains(&t) {
        right.page = Page::Joined;
        right.room_name = "房间 A216".into();
        right.selected = 0;
        right.phase = match t {
            t if t < 7.6 => Phase::Connecting,
            t if t < 8.5 => Phase::ApprovalPendingClient,
            t if t < 9.4 => Phase::Checking,
            t if t < 10.0 => Phase::Notifying,
            _ => Phase::WaitingStart,
        };
        left.phase = match t {
            t if t < 7.6 => Phase::WaitingPeer,
            t if t < 8.5 => Phase::ApprovalPending,
            t if t < 10.0 => Phase::PeerPreparing,
            _ => Phase::HostReady,
        };
        left.ready = t >= 10.0;
        right.ready = t >= 10.0;
        right.busy = t < 10.0;
    }
    if (12.6..15.5).contains(&t) {
        for (index, view) in [&mut left, &mut right].into_iter().enumerate() {
            view.network_only = true;
            view.title.clear();
            view.core.clear();
            view.selected = 0;
            view.page = if t < 13.5 {
                Page::Mode
            } else if index == 0 {
                Page::Host
            } else {
                Page::Joined
            };
            if t >= 13.5 {
                view.phase = Phase::NetworkConnected;
                view.connected = true;
                view.room_name = "房间 A216".into();
                view.local_address = if index == 0 {
                    "192.168.49.1"
                } else {
                    "192.168.49.2"
                }
                .into();
                view.peers.push(NetworkPeer {
                    name: "好友".into(),
                    address: if index == 0 {
                        "192.168.49.2"
                    } else {
                        "192.168.49.1"
                    }
                    .into(),
                });
            }
        }
    }
    let palette = if (16.2..17.0).contains(&t) {
        Palette::Sunset
    } else if (17.0..18.0).contains(&t) {
        Palette::Night
    } else {
        Palette::Forest
    };
    if (15.5..18.0).contains(&t) {
        left.selected = usize::from((16.0..17.0).contains(&t));
        right.selected = 1 - left.selected;
    }
    let host_events = [2.45, 8.45, 13.3, 16.2, 17.0, 18.0];
    let guest_events = [5.15, 6.95, 13.3, 16.2, 17.0, 18.0];
    let host_focus = envelope(t, 1.5, 4.5) + envelope(t, 7.5, 9.0);
    let guest_focus = envelope(t, 4.5, 7.5);
    let focus = host_focus - guest_focus;
    let float =
        (t * std::f32::consts::TAU / 5.0).sin() * (1.0 - smooth(t / 1.5) + envelope(t, 18.0, 19.0));
    let poses = [
        Pose {
            x: 300.0 + 28.0 * focus,
            y: 440.0 + 5.0 * float - 28.0 * host_focus,
            scale: 0.93 + 0.18 * host_focus - 0.18 * guest_focus,
            angle: -5.0 + 7.0 * host_focus - 3.0 * guest_focus + float,
        },
        Pose {
            x: 820.0 + 28.0 * focus,
            y: 440.0 - 5.0 * float - 28.0 * guest_focus,
            scale: 0.93 + 0.18 * guest_focus - 0.18 * host_focus,
            angle: 5.0 - 7.0 * guest_focus + 3.0 * host_focus - float,
        },
    ];
    let captions = [
        ("和好友一起玩", howff_menu::brand::NAME),
        ("创建房间", "01   OPEN A ROOM"),
        ("发现附近的好友", "02   NEARBY ROOMS"),
        ("确认加入", "03   YOU'RE INVITED"),
        ("准备好了，一起出发", "04   READY, PLAYER TWO"),
        ("仅组网，也能一起玩", "LOCAL NETWORK  /  KEEP CONNECTED"),
        ("换个主题，继续冒险", "FOREST  /  SUNSET  /  NIGHT"),
    ];
    Shot {
        screens: [
            Screen {
                view: left,
                palette,
                press: host_events.iter().filter(|event| t >= **event).count() as u8,
            },
            Screen {
                view: right,
                palette,
                press: guest_events.iter().filter(|event| t >= **event).count() as u8,
            },
        ],
        frame: Frame {
            time: t,
            poses,
            presses: [pulse(t, &host_events), pulse(t, &guest_events)],
            title: captions[stage].0,
            subtitle: captions[stage].1,
            stage,
            connected: (10.0..15.5).contains(&t),
        },
    }
}
