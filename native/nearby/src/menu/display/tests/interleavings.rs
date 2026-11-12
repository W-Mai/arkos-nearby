use super::*;
use crate::catalog::interleavings::Fixture;
use crate::flow::{Flow, Operation, Page};
#[cfg(target_os = "linux")]
use std::fs;
use std::sync::mpsc;

struct Gate {
    release: mpsc::Sender<()>,
    worker: thread::JoinHandle<()>,
}

impl Gate {
    fn event(replies: Replies, event: Event) -> Self {
        let (release, waiting) = mpsc::channel();
        let worker = thread::spawn(move || {
            waiting.recv_timeout(Duration::from_secs(10)).unwrap();
            replies.send(event);
        });
        Self { release, worker }
    }

    fn deliver(self) {
        self.release.send(()).unwrap();
        self.worker.join().unwrap();
    }
}

fn present(display: &mut Display, browser: &crate::catalog::Browser) {
    let mut view = browser.view();
    view["_test_messages"] = json!([]);
    display
        .present_in(view, browser.input_epoch.to_bits())
        .unwrap();
}

#[test]
fn old_search_replies_cannot_release_or_replace_a_new_screen_search() {
    let fixture = Fixture::new();
    let mut display = display();
    let old_replies = display.begin_screen();
    let mut old_browser = fixture.search("alpha");
    present(&mut display, &old_browser);
    let old = Gate::event(
        old_replies,
        Event::Searched(old_browser.search_job().unwrap().run()),
    );

    let current_replies = display.begin_screen();
    let mut current = fixture.search("beta");
    let result = current.search_job().unwrap().run();
    present(&mut display, &current);
    let before = current.view();
    old.deliver();
    assert!(display.receive().unwrap().is_none());
    assert_eq!(current.view(), before);
    assert_eq!(current.view()["library"]["load"]["state"], "loading");
    current_replies.send(Event::Searched(result));
    let Event::Searched(result) = next(&display) else {
        panic!("Missing current result")
    };
    current.searched(result);
    assert_eq!(
        current.current().unwrap().path(),
        fixture.root.join("nes/BetaTwo.nes")
    );
}

#[test]
fn queued_prepare_results_cannot_choose_a_game_in_the_next_browser() {
    let fixture = Fixture::new();
    for failed in [false, true] {
        let mut display = display();
        let replies = display.begin_screen();
        let mut previous = fixture.browser();
        previous.dispatch("confirm").unwrap();
        previous.dispatch("confirm").unwrap();
        present(&mut display, &previous);
        previous.dispatch("back").unwrap();
        let result = if failed {
            Err("Retired preparation failed".into())
        } else {
            Ok(Box::new(fixture.model_context("AlphaOne.nes")))
        };
        let old = Gate::event(replies, Event::Prepared(result));

        let current_replies = display.begin_screen();
        let mut current = fixture.browser();
        current.dispatch("confirm").unwrap();
        current.dispatch("down").unwrap();
        let selected = current.dispatch("confirm").unwrap().unwrap();
        present(&mut display, &current);
        let before = current.view();
        old.deliver();
        assert!(display.receive().unwrap().is_none());
        assert_eq!(current.view(), before);
        assert!(current.busy && current.selected_context.is_none());
        current_replies.send(Event::Prepared(Ok(Box::new(
            fixture.model_context("BetaTwo.nes"),
        ))));
        let Event::Prepared(result) = next(&display) else {
            panic!("Missing current preparation")
        };
        current.prepared(result.map(|value| *value));
        assert_eq!(
            current.selected_context.as_ref().unwrap().game_path,
            selected.path()
        );
    }
}

#[test]
fn canceled_game_selection_recovers_its_session_before_reusing_a_new_flow() {
    let fixture = Fixture::new();
    let mut display = display();
    let previous_replies = display.begin_screen();
    let mut flow = Flow::new(Some(fixture.model_context("AlphaOne.nes")));
    let id = "a".repeat(32);
    flow.status(
        flow.generation,
        json!({
            "id":id,"role":"client","phase":"joining","step":"selecting_game",
            "context_matches":true,"network_restored":false,
            "candidates":[{"key":"local","available":true,"name":"BetaTwo"}]
        }),
    );
    assert_eq!(flow.page, Page::Candidates);
    flow.dispatch("confirm", crate::system::monotonic() + 1.0);
    let request = flow.request.take().unwrap();
    assert_eq!(request.operation, Operation::SelectGame);
    let delayed = Gate::event(
        previous_replies.clone(),
        Event::Action(
            request,
            json!({
                "id":id,"role":"client","phase":"ready","step":"client_ready","ready":true
            }),
            false,
        ),
    );
    let retired_generation = flow.generation;
    flow.dispatch("back", crate::system::monotonic() + 1.0);
    flow.dispatch("back", crate::system::monotonic() + 1.0);
    assert_eq!(flow.page, Page::Leaving);
    assert!(flow.request.is_none() && flow.exit_code.is_none());
    delayed.deliver();
    let Event::Action(request, result, failed) = next(&display) else {
        panic!("Missing owned action")
    };
    flow.complete(&request, result, failed);
    assert_eq!(flow.page, Page::Leaving);
    let cancel = flow.request.take().unwrap();
    assert_eq!(cancel.operation, Operation::Cancel);
    assert_eq!(cancel.payload["session_id"], id);
    assert!(flow.exit_code.is_none());
    flow.complete(
        &cancel,
        json!({
            "id":id,"phase":"closed","network_restored":true,"recovery_errors":[]
        }),
        false,
    );
    assert_eq!(flow.page, Page::Rooms);
    assert_eq!(flow.view()["ready"], false);

    let current_replies = display.begin_screen();
    let mut current = Flow::new(Some(fixture.model_context("BetaTwo.nes")));
    let before = current.view();
    previous_replies.send(Event::Action(
        request,
        json!({"id":id,"phase":"playing","ready":true}),
        false,
    ));
    previous_replies.send(Event::Status(
        retired_generation,
        json!({"id":id,"phase":"playing","ready":true}),
    ));
    previous_replies.send(Event::Scan(
        retired_generation,
        json!({"rooms":[{"id":"old"}]}),
    ));
    for _ in 0..3 {
        assert!(display.receive().unwrap().is_none());
    }
    assert_eq!(current.view(), before);
    assert_eq!(current.view()["title"], "BetaTwo.nes");
    current.dispatch("confirm", crate::system::monotonic() + 1.0);
    let create = current.request.take().unwrap();
    current_replies.send(Event::Action(
        create,
        json!({
            "id":"b".repeat(32),"role":"host","phase":"hosting","step":"waiting_peer"
        }),
        false,
    ));
    let Event::Action(request, result, failed) = next(&display) else {
        panic!("Missing current action")
    };
    current.complete(&request, result, failed);
    assert_eq!(current.page, Page::Host);
    assert_eq!(current.view()["ready"], false);
}

#[test]
#[cfg(target_os = "linux")]
#[ignore = "Requires the handheld GPU, input devices and an owned foreground test stage"]
fn handheld_gpu_index_completion_keeps_focus() {
    let stage = std::path::PathBuf::from(
        std::env::var_os("HOWFF_INTERLEAVING_STAGE").expect("Owned test stage"),
    );
    assert_eq!(
        stage.parent(),
        Some(std::path::Path::new("/run/howff-tests"))
    );
    assert!(crate::protocol::hex(
        stage.file_name().unwrap().to_str().unwrap(),
        32
    ));
    let fixture = Fixture::new();
    let browser = fixture.browser();
    fs::write(fixture.root.join("nes/Aardvark.nes"), b"index fixture").unwrap();
    let refreshed = fixture.job.clone().run();
    let release = stage.join("release-index");
    let scenario = json!({
        "selected_game":fixture.root.join("nes/BetaTwo.nes"),
        "new_game":fixture.root.join("nes/Aardvark.nes")
    });
    fs::write(
        stage.join("scenario.json"),
        serde_json::to_vec(&scenario).unwrap(),
    )
    .unwrap();
    let index_task = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(90);
        while !release.is_file() {
            assert!(
                Instant::now() < deadline,
                "Index test gate was not released"
            );
            thread::sleep(Duration::from_millis(10));
        }
        Some(refreshed)
    });
    let mut menu = super::super::super::MenuSession {
        display: Display::open().unwrap(),
        lobby_flow: None,
        launcher: false,
        index_task: Some(index_task),
        index_invalidated: false,
    };
    let mut browser = browser;
    let result = menu.choose_game(&mut browser).unwrap();
    assert!(result.is_none());
    assert!(browser.selected_context.is_none());
    fs::write(stage.join("ui-finished.json"), b"{\"finished\":true}").unwrap();
}
