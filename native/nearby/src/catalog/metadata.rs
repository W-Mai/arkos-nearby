use std::path::PathBuf;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Metadata {
    pub name: String,
    pub image: Option<PathBuf>,
}
