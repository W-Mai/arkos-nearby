use crate::details::Fact;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    #[default]
    States,
    Sram,
    Session,
    Backups,
}
impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Self::States => "即时存档",
            Self::Sram => "游戏进度",
            Self::Session => "联机",
            Self::Backups => "存档备份",
        }
    }
    pub fn cycle(self, step: i32) -> Self {
        let tabs = [Self::States, Self::Sram, Self::Session];
        let index = tabs.iter().position(|t| *t == self).unwrap_or(0) as i32;
        tabs[(index + step).rem_euclid(3) as usize]
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    State,
    Sram,
    Backup,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceMatch {
    Matched,
    Unrecorded,
    Conflict,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub key: String,
    pub kind: Kind,
    pub slot: Option<i32>,
    pub title: String,
    pub path: String,
    pub bytes: u64,
    pub modified: u64,
    pub time_label: String,
    pub revision: String,
    pub thumbnail: String,
    pub thumbnail_size: (u32, u32),
    pub thumbnail_revision: Option<(u64, Option<std::time::SystemTime>)>,
    pub source: SourceMatch,
    pub core: String,
}
impl Record {
    pub fn tab(&self) -> Tab {
        match self.kind {
            Kind::State => Tab::States,
            Kind::Sram => Tab::Sram,
            Kind::Backup if self.slot.is_none() => Tab::Sram,
            Kind::Backup => Tab::Backups,
        }
    }
    pub fn can(&self, operation: Operation) -> bool {
        match operation {
            Operation::Backup => self.kind != Kind::Backup,
            Operation::Delete => self.kind != Kind::Sram,
            Operation::Restore => self.kind == Kind::Backup && self.source != SourceMatch::Conflict,
            Operation::Load => {
                self.kind == Kind::State
                    && self.source != SourceMatch::Conflict
                    && self.bytes > 0
                    && self.bytes <= 256 * 1024 * 1024
            }
            Operation::Start => self.kind == Kind::Sram,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Backup,
    Delete,
    Restore,
    Load,
    Start,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Document {
    pub title: String,
    pub core: String,
    pub revision: String,
    pub records: Vec<Record>,
    pub session: Vec<Fact>,
    pub room_active: bool,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct View {
    pub epoch: u64,
    pub document: Option<Document>,
    pub busy: bool,
    pub error: String,
    pub notice: String,
    pub completed: u64,
    pub return_tab: Tab,
    pub return_key: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Close {
        epoch: u64,
    },
    Reload {
        epoch: u64,
    },
    Manage {
        epoch: u64,
        key: String,
        revision: String,
        operation: Operation,
        tab: Tab,
    },
    Start {
        epoch: u64,
        tab: Tab,
    },
}
impl Request {
    pub fn epoch(&self) -> u64 {
        match self {
            Self::Close { epoch }
            | Self::Reload { epoch }
            | Self::Manage { epoch, .. }
            | Self::Start { epoch, .. } => *epoch,
        }
    }
    pub fn valid(&self) -> bool {
        match self {
            Self::Manage { key, revision, .. } => {
                !key.is_empty() && key.len() <= 128 && revision.len() == 64
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_actions_are_derived_from_owned_record_kinds() {
        let mut r = Record {
            key: "x".into(),
            kind: Kind::Sram,
            slot: None,
            title: String::new(),
            path: String::new(),
            bytes: 1,
            modified: 0,
            time_label: String::new(),
            revision: String::new(),
            thumbnail: String::new(),
            thumbnail_size: (0, 0),
            thumbnail_revision: None,
            source: SourceMatch::Unrecorded,
            core: String::new(),
        };
        assert!(r.can(Operation::Backup));
        assert!(!r.can(Operation::Delete));
        assert!(!r.can(Operation::Load));
        r.kind = Kind::State;
        assert!(r.can(Operation::Load));
        r.source = SourceMatch::Conflict;
        assert!(!r.can(Operation::Load));
        r.kind = Kind::Backup;
        assert!(!r.can(Operation::Restore));
        assert!(!r.can(Operation::Backup));
    }
}
