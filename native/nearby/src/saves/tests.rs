use super::*;
use std::os::unix::fs::PermissionsExt;

struct Fixture {
    root: PathBuf,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nearby-save-{}",
            crate::system::random_id().unwrap()
        ));
        fs::create_dir_all(root.join("game")).unwrap();
        let paths = ResolvedSavePaths {
            sram: root.join("game/progress.srm"),
            state: root.join("game/game.state"),
            slots: Default::default(),
            content_directory: true,
        };
        let store = Store::new(
            &root.join("backups"),
            Identity {
                content: "a".repeat(64),
                core: "b".repeat(64),
                bits: 64,
                name: "Test 1".into(),
            },
            paths,
            "Game".into(),
        )
        .unwrap();
        fs::write(root.join("game/progress.srm"), b"original progress").unwrap();
        fs::write(root.join("game/game.state1"), b"state one").unwrap();
        fs::write(
            root.join("game/game.state1.png"),
            [
                137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0,
                1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 248,
                15, 4, 0, 9, 251, 3, 253, 251, 94, 107, 43, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
                96, 130,
            ],
        )
        .unwrap();
        Self { root, store }
    }
    fn find(&mut self, kind: Kind) -> Record {
        self.store
            .scan()
            .unwrap()
            .records
            .into_iter()
            .find(|r| r.kind == kind)
            .unwrap()
    }
    fn operate(&mut self, record: &Record, op: Operation) -> Result<Option<launch::Intent>> {
        self.store.operate(&record.key, &record.revision, op)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn typed_paths_index_only_real_slots_and_never_promote_unknown_state_sources() {
    let mut f = Fixture::new();
    fs::write(f.root.join("game/game.state01"), b"not slot one").unwrap();
    fs::write(f.root.join("game/game.state0"), b"not slot zero").unwrap();
    fs::write(f.root.join("game/game.state1.bak"), b"not a slot").unwrap();
    let document = f.store.scan().unwrap();
    assert_eq!(document.records.len(), 2);
    assert!(
        document
            .records
            .iter()
            .all(|r| r.source == SourceMatch::Unrecorded)
    );
    let state = f.find(Kind::State);
    f.operate(&state, Operation::Backup).unwrap();
    let backup = f.find(Kind::Backup);
    assert_eq!(backup.source, SourceMatch::Unrecorded);
    assert!(!backup.thumbnail.is_empty());
    assert!(files::solo(Path::new("/run/arkos-nearby-rust/room/game/saves/data.srm")).is_err());
}

#[test]
fn stale_files_cannot_be_backed_up_deleted_or_loaded_and_progress_is_not_deletable() {
    let mut f = Fixture::new();
    let state = f.find(Kind::State);
    fs::write(&state.path, b"external state").unwrap();
    for op in [Operation::Backup, Operation::Delete, Operation::Load] {
        assert!(f.operate(&state, op).is_err());
    }
    assert_eq!(fs::read(&state.path).unwrap(), b"external state");
    let progress = f.find(Kind::Sram);
    assert!(f.operate(&progress, Operation::Delete).is_err());
}

#[test]
fn restore_keeps_original_progress_metadata_and_symlink_and_delete_preserves_a_backup() {
    let mut f = Fixture::new();
    let real = f.root.join("game/real.srm");
    fs::rename(&f.store.paths.sram, &real).unwrap();
    std::os::unix::fs::symlink(&real, &f.store.paths.sram).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
    let progress = f.find(Kind::Sram);
    f.operate(&progress, Operation::Backup).unwrap();
    let backup = f.find(Kind::Backup);
    fs::write(&real, b"newer progress").unwrap();
    f.store.scan().unwrap();
    assert_eq!(
        f.operate(&backup, Operation::Restore).unwrap(),
        Some(launch::Intent::Sram)
    );
    assert_eq!(fs::read(&real).unwrap(), b"original progress");
    assert!(
        f.store
            .paths
            .sram
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::metadata(&real).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let state = f.find(Kind::State);
    f.operate(&state, Operation::Delete).unwrap();
    assert!(!Path::new(&state.path).exists());
    assert!(!Path::new(&state.thumbnail).exists());
    let records = f.store.scan().unwrap().records;
    assert!(
        records
            .iter()
            .any(|r| r.kind == Kind::Backup && r.slot == Some(1))
    );
}

#[test]
fn interrupted_restore_recovers_only_owned_bytes_and_preserves_external_edits() {
    let mut f = Fixture::new();
    let before = f.find(Kind::Sram);
    let id_before = f.store.capture(&before).unwrap();
    fs::write(&before.path, b"replacement").unwrap();
    let after = f.find(Kind::Sram);
    let id_after = f.store.capture(&after).unwrap();
    let journal = Journal {
        target: f.store.paths.sram.clone(),
        before: Some(id_before),
        source: id_after,
        slot: None,
    };
    crate::system::write(&f.store.root.join("transaction.json"), &journal).unwrap();
    f.store.recover().unwrap();
    assert_eq!(fs::read(&before.path).unwrap(), b"original progress");
    crate::system::write(&f.store.root.join("transaction.json"), &journal).unwrap();
    fs::write(&before.path, b"external progress").unwrap();
    assert!(f.store.recover().is_err());
    assert_eq!(fs::read(&before.path).unwrap(), b"external progress");
}

#[test]
fn image_decode_failure_does_not_remove_the_paired_file_from_a_backup() {
    let mut f = Fixture::new();
    let image = f.root.join("game/game.state1.png");
    fs::write(&image, b"damaged image bytes").unwrap();
    let state = f.find(Kind::State);
    assert!(state.thumbnail.is_empty());
    f.operate(&state, Operation::Delete).unwrap();
    assert!(!image.exists());
    let backup = f.find(Kind::Backup);
    f.operate(&backup, Operation::Restore).unwrap();
    assert_eq!(fs::read(image).unwrap(), b"damaged image bytes");
}
