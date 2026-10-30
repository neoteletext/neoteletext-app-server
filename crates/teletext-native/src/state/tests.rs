use super::*;

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("teletext-state-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    folder
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
