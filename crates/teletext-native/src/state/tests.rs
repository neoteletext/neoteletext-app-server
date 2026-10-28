use super::*;

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("teletext-state-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    folder
}

/// What a channel keeps is written under its keys and read back; what it
/// forgets is gone; a file that is no key is named and left alone
/// (requirement 5.2).
#[test]
fn records_are_kept_read_back_and_forgotten() {
    let state = StateDirectory::open(&scratch("records")).unwrap();
    let mut changes = Changes::new();
    changes.insert(StoreKey::Page(0x100), Some(b"front".to_vec()));
    changes.insert(StoreKey::Pages(0), Some(b"pages".to_vec()));
    state.apply("hackaday", &changes).unwrap();
    fs::write(state.root().join("channels/hackaday/notes.txt"), b"mine").unwrap();
    let (records, ignored) = state.records("hackaday");
    assert_eq!(records, [(StoreKey::Page(0x100), b"front".to_vec()), (StoreKey::Pages(0), b"pages".to_vec())]);
    assert_eq!(ignored, ["notes.txt"]);
    let mut forget = Changes::new();
    forget.insert(StoreKey::Page(0x100), None);
    forget.insert(StoreKey::Page(0x200), None);
    state.apply("hackaday", &forget).unwrap();
    assert_eq!(state.records("hackaday").0, [(StoreKey::Pages(0), b"pages".to_vec())]);
    assert_eq!(state.records("elsewhere").0, []);
    assert!(!state.root().join("channels/hackaday/page-100.new").exists(), "no temporary file left");
}

/// The session key is made once, kept for its owner alone, and the same
/// at the next start; a previous key is read when there is one
/// (requirement 5.5).
#[test]
fn the_session_key_is_made_once_and_kept() {
    let root = scratch("keys");
    let state = StateDirectory::open(&root).unwrap();
    let (key, previous) = state.session_keys().unwrap();
    assert_eq!(previous, None);
    assert_eq!(StateDirectory::open(&root).unwrap().session_keys().unwrap().0, key);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(root.join(KEY_FILE)).unwrap().permissions().mode() & 0o777, OWNER_ONLY_FILE);
        assert_eq!(fs::metadata(&root).unwrap().permissions().mode() & 0o777, OWNER_ONLY_FOLDER);
    }
    fs::write(root.join(PREVIOUS_KEY_FILE), [3_u8; KEY_BYTES]).unwrap();
    assert_eq!(state.session_keys().unwrap(), (key, Some([3; KEY_BYTES])));
    fs::write(root.join(PREVIOUS_KEY_FILE), b"short").unwrap();
    assert!(state.session_keys().is_err(), "a key of the wrong length is refused");
}

/// Figures are kept and read back; a file that does not read is kept
/// aside and the figures start again (requirement 8.3).
#[test]
fn figures_are_kept_and_an_unreadable_file_is_kept_aside() {
    let root = scratch("figures");
    let state = StateDirectory::open(&root).unwrap();
    assert!(state.figures().unwrap().is_none());
    state.keep_figures(b"{\"type\":\"figures\",\"version\":1}\n").unwrap();
    assert!(state.figures().unwrap().is_some_and(|figures| figures.get("version").and_then(json::Value::integer) == Some(1)));
    fs::write(root.join(FIGURES_FILE), b"{not json").unwrap();
    let error = state.figures().unwrap_err();
    assert!(error.contains("kept as"), "{error}");
    assert_eq!(fs::read(root.join(UNREADABLE_FIGURES)).unwrap(), b"{not json");
    assert!(state.figures().unwrap().is_none(), "the figures start again");
}
