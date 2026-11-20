use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::context::{self, Context};
use crate::discovery::LocalGame;
use crate::protocol::{Profile, VerifiedRoom};
use crate::session::{self, State};
use crate::system::{monotonic, read, write};
use crate::{Result, require};

#[derive(Serialize, Deserialize)]
pub(crate) struct Candidate {
    pub key: String,
    pub name: String,
    pub detail: String,
    pub context: Option<Context>,
}

pub(crate) fn select(state: &State, key: &str) -> Result<()> {
    require(
        state.role == "client" && state.step == "selecting_game",
        "No active game selection",
    )?;
    let candidates: Vec<Candidate> = read(&state.root().join("candidates.json"))?;
    require(
        candidates
            .iter()
            .any(|candidate| candidate.key == key && candidate.context.is_some()),
        "Candidate is unavailable",
    )?;
    write(
        &state.root().join("selected-game.json"),
        &json!({"session_id":state.id,"key":key}),
    )
}

fn check(state: &State) -> Result<()> {
    let current = session::current(&state.id)?;
    require(
        !current.closed() && current.phase != "restoring" && monotonic() < current.deadline,
        "Local game search canceled or expired",
    )
}

fn publish(state: &State, candidates: &[Candidate]) -> Result<()> {
    check(state)?;
    write(&state.root().join("candidates.json"), &candidates)
}

pub(crate) fn resolve(
    state: &State,
    room: &VerifiedRoom,
    profile: &Profile,
    preferred: Option<&Context>,
    preview: Option<&LocalGame>,
) -> Result<Context> {
    profile.validate(room)?;
    if let Some(matched) = preferred.and_then(|context| context.matching(room, profile).ok()) {
        return Ok(matched);
    }
    session::progress(
        &state.id,
        None,
        "resolving_game",
        "正在查找本机的对应游戏...",
    )?;
    let mut candidates = Vec::new();
    let result = find(state, room, profile, preview, &mut candidates);
    if let Err(error) = &result {
        write(
            &state.root().join("match-error.json"),
            &json!({"detail":error.to_string()}),
        )?;
    }
    publish(state, &candidates)?;
    if !state.root().join("selected-game.json").is_file() {
        session::progress(
            &state.id,
            None,
            "selecting_game",
            if candidates
                .iter()
                .any(|candidate| candidate.context.is_some())
            {
                "已找到对应游戏，选择后继续加入。"
            } else {
                "本机没有可用的对应游戏，按 B 返回并恢复 Wi-Fi。"
            },
        )?;
    }
    let mut next_check = 0.0;
    loop {
        check(state)?;
        if let Ok(selected) = read::<serde_json::Value>(&state.root().join("selected-game.json")) {
            require(
                selected["session_id"] == state.id,
                "Selection belongs to another session",
            )?;
            let context = candidates
                .iter()
                .find(|candidate| selected["key"] == candidate.key)
                .and_then(|candidate| candidate.context.as_ref())
                .ok_or("Selected game is no longer available")?;
            return context.matching(room, profile);
        }
        if monotonic() >= next_check {
            let value: serde_json::Value =
                serde_json::from_slice(&crate::http::request("/room-status", None)?)?;
            require(
                value["session_id"] == room.room().session_id,
                "Selected room closed during game selection",
            )?;
            next_check = monotonic() + 2.0;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn inventory() -> Result<Vec<super::Entry>> {
    Ok(super::IndexJob::local()?.run().entries().cloned().collect())
}

struct LocalMatch<'a> {
    room: &'a VerifiedRoom,
    profile: &'a Profile,
    core: context::Core,
    library: PathBuf,
    roots: Vec<PathBuf>,
    title: String,
}

impl LocalMatch<'_> {
    fn title_matches(&self, path: &Path, name: &str) -> bool {
        name.to_lowercase() == self.title
            || path
                .file_stem()
                .is_some_and(|name| name.to_string_lossy().to_lowercase() == self.title)
    }

    fn candidate(&self, path: &Path, name: &str) -> Result<Option<Candidate>> {
        let extension = path.extension().unwrap_or_default().to_string_lossy();
        let archive_member = extension.eq_ignore_ascii_case("zip") && !self.core.block_extract;
        let possible = archive_member
            || self
                .core
                .extensions
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(&extension))
                && path
                    .metadata()
                    .is_ok_and(|file| file.len() == self.room.room().content.size);
        let matches = possible
            && crate::content::fingerprint(
                path,
                &self.core.extensions,
                &self.roots,
                self.core.block_extract,
            )
            .is_ok_and(|content| content.same_bytes(&self.room.room().content));
        if !matches && !self.title_matches(path, name) {
            return Ok(None);
        }
        self.row(name, matches, matches.then(|| self.context(path)).flatten())
            .map(Some)
    }

    fn context(&self, path: &Path) -> Option<Context> {
        context::parse_selected(
            &self.profile.frontend,
            &[
                "-L".into(),
                self.library.to_string_lossy().into(),
                path.to_string_lossy().into(),
            ],
            false,
        )
        .and_then(|context| {
            context.check_match(self.room, self.profile)?;
            Ok(context)
        })
        .ok()
    }

    fn row(&self, name: &str, matches: bool, context: Option<Context>) -> Result<Candidate> {
        Ok(Candidate {
            key: crate::system::random_id()?,
            name: crate::flow::label(name),
            detail: if context.is_some() {
                format!("{} · 游戏内容匹配", self.core.identity.name)
            } else if matches {
                "游戏内容相同，模拟器或核心版本不同".into()
            } else {
                "同名游戏，内容版本不同".into()
            },
            context,
        })
    }
}

fn publish_candidate(
    state: &State,
    candidates: &mut Vec<Candidate>,
    candidate: Candidate,
) -> Result<()> {
    candidates.push(candidate);
    publish(state, candidates)?;
    if candidates.len() == 1 {
        session::progress(
            &state.id,
            None,
            "selecting_game",
            "已发现本地游戏，仍在查找其他版本...",
        )?;
    }
    Ok(())
}

fn find(
    state: &State,
    room: &VerifiedRoom,
    profile: &Profile,
    preview: Option<&LocalGame>,
    candidates: &mut Vec<Candidate>,
) -> Result<()> {
    let library = context::resolve_core(&profile.frontend, &profile.core_id)
        .map_err(|_| "本机缺少房间使用的核心")?;
    let core = context::inspect(&library, &profile.frontend)?;
    require(
        core.sha256 == profile.core_sha256 && core.bits == profile.elf_bits,
        "本机核心版本与房间不同",
    )?;
    let search = LocalMatch {
        room,
        profile,
        core,
        library,
        roots: crate::platform::content::roots()?,
        title: room.room().content.title.to_lowercase(),
    };
    check(state)?;
    let mut matched_preview = None;
    if let Some(preview) = preview
        && let Some(context) = search.context(&preview.path)
    {
        let candidate = search.row(&preview.name, true, Some(context))?;
        publish_candidate(state, candidates, candidate)?;
        matched_preview = Some(preview.path.as_path());
    }
    if state.root().join("selected-game.json").is_file() {
        return Ok(());
    }

    // The advertised path only prioritizes work. Check the rest of the
    // inventory so renamed copies and alternative content stay discoverable.
    let entries = inventory()?;
    check(state)?;
    let metadata: std::collections::BTreeMap<_, _> = entries
        .iter()
        .map(|entry| (entry.path.as_path(), entry))
        .collect();
    let mut paths: Vec<_> = metadata.keys().copied().collect();
    paths.sort_by_key(|path| !search.title_matches(path, &metadata[path].name));
    for path in paths {
        check(state)?;
        if state.root().join("selected-game.json").is_file() {
            break;
        }
        if matched_preview == Some(path) {
            continue;
        }
        let entry = metadata[path];
        let name = if entry.name.is_empty() {
            path.file_stem().unwrap_or_default().to_string_lossy()
        } else {
            std::borrow::Cow::Borrowed(entry.name.as_str())
        };
        if let Some(candidate) = search.candidate(path, &name)? {
            publish_candidate(state, candidates, candidate)?;
        }
    }
    Ok(())
}
