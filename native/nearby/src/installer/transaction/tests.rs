use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Fixture(Layout);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("howff-install-{}", random_id().unwrap()));
        fs::create_dir(&root).unwrap();
        let layout = Layout {
            prefix: root.canonicalize().unwrap(),
        };
        fs::create_dir_all(layout.path(ROOT).unwrap()).unwrap();
        for (name, bytes, mode) in [
            ("howff", b"old".as_slice(), 0o750),
            ("device.json", b"device".as_slice(), 0o640),
        ] {
            let path = layout.file(name).unwrap();
            fs::write(&path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
        }
        Self(layout)
    }
    fn changes(&self) -> Vec<Change> {
        vec![
            Change {
                target: format!("{ROOT}/howff"),
                data: Some(b"updated".to_vec()),
                mode: 0o755,
            },
            Change {
                target: format!("{ROOT}/device.json"),
                data: None,
                mode: 0o600,
            },
            Change {
                target: format!("{ROOT}/identity.json"),
                data: Some(b"identity".to_vec()),
                mode: 0o600,
            },
        ]
    }
    fn snapshot(&self) -> Vec<(Option<Image>, Vec<u8>)> {
        ["howff", "device.json", "identity.json"]
            .into_iter()
            .map(|name| Image::capture(&self.0.file(name).unwrap()).unwrap())
            .collect()
    }
    fn clean(&self) {
        assert!(!self.0.file(JOURNAL).unwrap().exists());
        for item in fs::read_dir(self.0.path(ROOT).unwrap()).unwrap() {
            assert!(!valid_directory(
                &item.unwrap().file_name().to_string_lossy()
            ));
        }
    }
    fn crash(&self, step: Step) {
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                transact_observed(&self.0, self.changes(), &mut |current| {
                    if current == step {
                        panic!("Interrupted at {current:?}");
                    }
                    Ok(())
                })
                .unwrap();
            }))
            .is_err()
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0.prefix);
    }
}

#[test]
fn complete_preflight_rejects_conflicts_without_staging_or_changing_files() {
    use std::os::unix::fs::symlink;
    for kind in 0..5 {
        let f = Fixture::new();
        let before = f.snapshot();
        let mut changes = f.changes();
        match kind {
            0 => changes.push(Change {
                target: "etc/unowned".into(),
                data: None,
                mode: 0o600,
            }),
            1 => changes.push(Change {
                target: format!("{ROOT}/howff"),
                data: None,
                mode: 0o600,
            }),
            2 => {
                fs::create_dir(f.0.file("identity.json").unwrap()).unwrap();
            }
            3 => {
                symlink(
                    f.0.file("howff").unwrap(),
                    f.0.file("identity.json").unwrap(),
                )
                .unwrap();
            }
            4 => {
                fs::create_dir(f.0.prefix.join("outside")).unwrap();
                symlink(f.0.prefix.join("outside"), f.0.file("assets").unwrap()).unwrap();
                changes.push(Change {
                    target: format!("{ROOT}/assets/8188eu.ko"),
                    data: Some(b"module".to_vec()),
                    mode: 0o600,
                });
            }
            _ => unreachable!(),
        }
        assert!(transact(&f.0, changes).is_err());
        assert_eq!(
            Image::capture(&f.0.file("howff").unwrap()).unwrap(),
            before[0]
        );
        assert_eq!(
            Image::capture(&f.0.file("device.json").unwrap()).unwrap(),
            before[1]
        );
        assert!(!f.0.prefix.join("outside/8188eu.ko").exists());
        f.clean();
    }
}

#[test]
fn errors_and_interruptions_restore_the_complete_previous_images() {
    for step in [
        Step::BackedUp(0),
        Step::BackedUp(1),
        Step::BackedUp(2),
        Step::Published,
        Step::Applied(0),
        Step::Applied(1),
        Step::Applied(2),
        Step::ReplacementCreated(0),
        Step::ReplacementWritten(0),
        Step::Replaced(0),
        Step::ReplacementCreated(2),
        Step::ReplacementWritten(2),
        Step::Replaced(2),
    ] {
        for abrupt in [false, true] {
            let f = Fixture::new();
            let before = f.snapshot();
            if abrupt {
                f.crash(step);
            } else {
                assert!(
                    transact_observed(&f.0, f.changes(), &mut |current| {
                        if current == step {
                            return Err("Injected I/O error".into());
                        }
                        Ok(())
                    })
                    .is_err()
                );
            }
            recover_transaction(&f.0).unwrap();
            assert_eq!(f.snapshot(), before, "Failed at {step:?}, abrupt={abrupt}");
            f.clean();
        }
    }
}

#[test]
fn durable_commit_is_retained_after_cleanup_interruption() {
    let f = Fixture::new();
    f.crash(Step::Committed);
    let committed = f.snapshot();
    assert_eq!(committed[0].1, b"updated");
    assert!(committed[1].0.is_none());
    recover_transaction(&f.0).unwrap();
    assert_eq!(f.snapshot(), committed);
    f.clean();
}

#[test]
fn an_error_after_the_commit_marker_reports_the_committed_result() {
    let f = Fixture::new();
    transact_observed(&f.0, f.changes(), &mut |step| {
        if step == Step::Committed {
            return Err("Cleanup I/O error".into());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(f.snapshot()[0].1, b"updated");
    f.clean();
}

#[test]
fn every_backup_and_target_is_validated_before_any_recovery_write() {
    for corrupt_backup in [true, false] {
        let f = Fixture::new();
        let before = f.snapshot();
        f.crash(Step::Applied(2));
        let journal: Journal = read(&f.0.file(JOURNAL).unwrap()).unwrap();
        let changed = if corrupt_backup {
            f.0.file(&format!("{}/0", journal.directory)).unwrap()
        } else {
            f.0.file("howff").unwrap()
        };
        let original = fs::read(&changed).unwrap();
        fs::write(&changed, b"external").unwrap();
        let interrupted = f.snapshot();
        assert!(recover_transaction(&f.0).is_err());
        assert_eq!(f.snapshot(), interrupted);
        fs::write(&changed, original).unwrap();
        recover_transaction(&f.0).unwrap();
        assert_eq!(f.snapshot(), before);
        f.clean();
    }
}

#[test]
fn interrupted_recovery_and_settled_cleanup_are_retryable() {
    for stop in [
        Step::Restored(0),
        Step::Restored(1),
        Step::Restored(2),
        Step::ReplacementCreated(0),
        Step::ReplacementWritten(0),
        Step::Replaced(0),
    ] {
        let f = Fixture::new();
        let before = f.snapshot();
        f.crash(Step::Applied(2));
        assert!(
            recover_observed(&f.0, &mut |step| {
                if step == stop {
                    return Err("Interrupted recovery".into());
                }
                Ok(())
            })
            .is_err()
        );
        recover_transaction(&f.0).unwrap();
        assert_eq!(f.snapshot(), before);
        f.clean();
    }
    for phase in [Phase::Committed, Phase::Restored] {
        let f = Fixture::new();
        f.crash(Step::Applied(2));
        let file = f.0.file(JOURNAL).unwrap();
        let mut journal: Journal = read(&file).unwrap();
        journal.phase = phase;
        publish(&f.0, &journal).unwrap();
        fs::remove_dir_all(f.0.file(&journal.directory).unwrap()).unwrap();
        let before = f.snapshot();
        recover_transaction(&f.0).unwrap();
        assert_eq!(f.snapshot(), before);
        f.clean();
    }
}
