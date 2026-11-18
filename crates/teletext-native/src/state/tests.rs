use super::*;

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("teletext-state-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    folder
}

/// VS-305, VS-603: the viewer-handle secret is made once, kept for its
/// owner alone, and the same at the next start; the session cookie's old
/// keys are deleted; a secret of the wrong length is refused.
#[test]
fn the_handle_secret_is_made_once_and_kept() {
    let root = scratch("secret");
    let state = StateDirectory::open(&root).unwrap();
    for retired in RETIRED_FILES {
        fs::write(root.join(retired), [3_u8; 32]).unwrap();
    }
    let (secret, rotated) = state.handle_secret().unwrap();
    assert!(!rotated);
    for retired in RETIRED_FILES {
        assert!(!root.join(retired).exists(), "{retired} is deleted");
    }
    assert_eq!(StateDirectory::open(&root).unwrap().handle_secret().unwrap().0.bytes(), secret.bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(root.join(HANDLE_SECRET_FILE)).unwrap().permissions().mode() & 0o777, OWNER_ONLY_FILE);
        assert_eq!(fs::metadata(&root).unwrap().permissions().mode() & 0o777, OWNER_ONLY_FOLDER);
    }
    fs::write(root.join(HANDLE_SECRET_FILE), b"short").unwrap();
    assert!(state.handle_secret().is_err(), "a secret of the wrong length is refused");
}

/// VS-306: rotation makes a new secret and marks it, until every program
/// was told.
#[test]
fn rotation_makes_a_new_secret_and_marks_it() {
    let root = scratch("rotation");
    let state = StateDirectory::open(&root).unwrap();
    let (before, _) = state.handle_secret().unwrap();
    state.rotate_handle_secret().unwrap();
    let (after, rotated) = state.handle_secret().unwrap();
    assert!(rotated);
    assert_ne!(after.bytes(), before.bytes());
    state.rotation_told().unwrap();
    assert!(!state.handle_secret().unwrap().1);
}

/// REQ-1054: figures are kept and read back; a file that does not read is
/// kept aside and the figures start again (requirement 8.3).
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
