use super::*;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
    pub(crate) job: IndexJob,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "howff-interleaving-{}",
            crate::system::random_id().unwrap()
        ));
        fs::create_dir_all(root.join("nes")).unwrap();
        let root = root.canonicalize().unwrap();
        for name in ["AlphaOne.nes", "BetaTwo.nes"] {
            // Index/search fixtures are not emulator content.
            fs::write(root.join("nes").join(name), name.as_bytes()).unwrap();
        }
        let platform = Entry {
            path: root.join("nes"),
            name: "NES".into(),
            directory: true,
            frontend: "retroarch".into(),
            core: "fceumm".into(),
            platform: "nes".into(),
            count: 0,
            image: None,
            extensions: ".nes".into(),
        };
        Self {
            job: IndexJob {
                roots: vec![root.clone()],
                platforms: vec![platform],
                cache: root.join("index.json"),
                seed: None,
            },
            root,
        }
    }

    pub(crate) fn browser(&self) -> Browser {
        let mut browser = Browser::with_inventory(self.job.clone(), true).unwrap();
        browser.indexed(self.job.clone().run());
        browser
    }

    pub(crate) fn search(&self, query: &str) -> Browser {
        let mut browser = self.browser();
        browser.dispatch("auxiliary").unwrap();
        Self::query(&mut browser, query);
        browser
    }

    pub(crate) fn query(browser: &mut Browser, query: &str) {
        browser.query = query.into();
        browser.changed_query();
    }

    pub(crate) fn model_context(&self, name: &str) -> Context {
        // This models result delivery; it does not inspect or load a frontend/core.
        let mut context = crate::contracts::context();
        context.title = name.into();
        context.game_path = self.root.join("nes").join(name);
        context.identity.content.title = name.into();
        context
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn delayed<T: Send + 'static>(value: T) -> (mpsc::Sender<()>, thread::JoinHandle<T>) {
    let (release, held) = mpsc::channel();
    (
        release,
        thread::spawn(move || {
            held.recv_timeout(Duration::from_secs(10)).unwrap();
            value
        }),
    )
}

#[test]
fn query_replacement_retires_results_without_losing_the_next_search() {
    let fixture = Fixture::new();
    let mut browser = fixture.search("alpha");
    let old = browser.search_job().unwrap().run();
    let (release, worker) = delayed(old);
    Fixture::query(&mut browser, "beta");
    assert!(
        browser.search_job().is_none(),
        "One worker remains in flight"
    );
    release.send(()).unwrap();
    browser.searched(worker.join().unwrap());
    assert!(browser.current().is_none());
    assert!(browser.queries.get("alpha").is_none());
    assert!(browser.query_due.is_some());
    let current = browser.search_job().unwrap().run();
    browser.searched(current);
    assert_eq!(
        browser.current().unwrap().path,
        fixture.root.join("nes/BetaTwo.nes")
    );
    assert!(browser.queries.get("beta").is_some());
    assert!(!browser.search_running && browser.query_due.is_none());
}

#[test]
fn search_cancel_and_reentry_never_publish_the_retired_query() {
    let fixture = Fixture::new();
    let mut browser = fixture.search("alpha");
    let (release, worker) = delayed(browser.search_job().unwrap().run());
    browser.dispatch("back").unwrap();
    browser.dispatch("back").unwrap();
    assert_eq!(browser.view()["library"]["kind"], "platforms");
    browser.dispatch("auxiliary").unwrap();
    Fixture::query(&mut browser, "beta");
    let before = browser.view();
    release.send(()).unwrap();
    browser.searched(worker.join().unwrap());
    assert_eq!(browser.view()["entries"], before["entries"]);
    assert_eq!(browser.view()["library"]["query"], "beta");
    assert_eq!(browser.view()["library"]["keyboard"], true);
    assert!(browser.queries.get("alpha").is_none());
    let current = browser.search_job().unwrap().run();
    browser.searched(current);
    assert_eq!(
        browser.current().unwrap().path,
        fixture.root.join("nes/BetaTwo.nes")
    );
}

#[test]
fn replaced_index_cannot_accept_ids_or_cache_entries_from_the_old_snapshot() {
    let fixture = Fixture::new();
    let mut browser = fixture.search("beta");
    let (release, worker) = delayed(browser.search_job().unwrap().run());
    fs::rename(
        fixture.root.join("nes/BetaTwo.nes"),
        fixture.root.join("nes/RenamedBeta.nes"),
    )
    .unwrap();
    browser.indexed(fixture.job.clone().run());
    release.send(()).unwrap();
    browser.searched(worker.join().unwrap());
    assert!(browser.current().is_none());
    assert!(browser.queries.get("beta").is_none());
    let current = browser.search_job().unwrap().run();
    browser.searched(current);
    assert_eq!(
        browser.current().unwrap().path,
        fixture.root.join("nes/RenamedBeta.nes")
    );
    assert!(browser.queries.get("beta").is_some());
}

#[test]
fn canceled_prepare_finishes_only_its_owned_busy_state_and_keeps_game_selection() {
    let fixture = Fixture::new();
    for failed in [false, true] {
        let mut browser = fixture.browser();
        browser.dispatch("confirm").unwrap();
        let entry = browser.dispatch("confirm").unwrap().unwrap();
        assert_eq!(entry.path, fixture.root.join("nes/AlphaOne.nes"));
        let result = if failed {
            Err("Retired preparation failed".into())
        } else {
            Ok(fixture.model_context("AlphaOne.nes"))
        };
        let (release, worker) = delayed(result);
        browser.dispatch("back").unwrap();
        for action in ["down", "confirm", "back", "up", "confirm"] {
            assert!(browser.dispatch(action).unwrap().is_none());
            assert_eq!(browser.current().unwrap().key(), entry.key());
            assert!(browser.selected_context.is_none());
        }
        release.send(()).unwrap();
        browser.prepared(worker.join().unwrap());
        assert!(!browser.busy && browser.selected_context.is_none());
        assert!(browser.status.is_empty());
        browser.dispatch("down").unwrap();
        let second = browser.dispatch("confirm").unwrap().unwrap();
        assert_eq!(second.path, fixture.root.join("nes/BetaTwo.nes"));
        browser.prepared(Ok(fixture.model_context("BetaTwo.nes")));
        assert_eq!(
            browser.selected_context.as_ref().unwrap().game_path,
            second.path
        );
    }
}

#[test]
fn delayed_index_preserves_the_game_selected_after_the_snapshot_was_built() {
    let fixture = Fixture::new();
    let mut browser = fixture.browser();
    browser.dispatch("confirm").unwrap();
    fs::write(fixture.root.join("nes/Aardvark.nes"), b"index fixture").unwrap();
    let (release, worker) = delayed(fixture.job.clone().run());
    browser.dispatch("down").unwrap();
    let selected = browser.current().unwrap();
    assert_eq!(selected.path, fixture.root.join("nes/BetaTwo.nes"));
    release.send(()).unwrap();
    browser.accept_index(worker.join().unwrap());
    assert_eq!(browser.current().unwrap().key(), selected.key());
    assert_eq!(browser.view()["library"]["position"], 3);
}

#[test]
fn delayed_index_uses_a_bounded_neighbor_when_the_selected_file_disappears() {
    let fixture = Fixture::new();
    let mut browser = fixture.browser();
    browser.dispatch("confirm").unwrap();
    browser.dispatch("down").unwrap();
    fs::remove_file(fixture.root.join("nes/BetaTwo.nes")).unwrap();
    let (release, worker) = delayed(fixture.job.clone().run());
    release.send(()).unwrap();
    browser.accept_index(worker.join().unwrap());
    assert_eq!(
        browser.current().unwrap().path,
        fixture.root.join("nes/AlphaOne.nes")
    );
    assert_eq!(browser.view()["library"]["position"], 1);
    assert_eq!(browser.view()["library"]["total"], 1);
}
