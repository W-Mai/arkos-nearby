use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    #[default]
    Overview,
    Core,
    Firmware,
    Saves,
    Storage,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Found,
    Missing,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    Required,
    Optional,
    Unspecified,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

impl Fact {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Row {
    pub name: String,
    pub summary: String,
    pub facts: Vec<Fact>,
    pub path: String,
    pub status: FileStatus,
    pub requirement: Option<Requirement>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Document {
    pub key: String,
    pub title: String,
    pub platform: String,
    pub core: Vec<Row>,
    pub firmware: Vec<Row>,
    pub saves: Vec<Row>,
    pub storage: Vec<Row>,
}

impl Document {
    pub fn rows(&self, section: Section) -> &[Row] {
        match section {
            Section::Overview => &[],
            Section::Core => &self.core,
            Section::Firmware => &self.firmware,
            Section::Saves => &self.saves,
            Section::Storage => &self.storage,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Load {
    #[default]
    Loading,
    Ready {
        document: Document,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct View {
    pub section: Section,
    pub load: Load,
    pub expanded: bool,
    pub scroll: u32,
}
