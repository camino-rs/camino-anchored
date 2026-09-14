// Copyright (c) The camino-anchored Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests for behavior fetching the current directory.
//!
//! These tests change the process cwd and rely on nextest's process-per-test
//! model. (Don't add workarounds for `cargo test`.)

#![cfg(any(unix, windows))]

use camino::Utf8Path;
use camino_anchored::{AbsUtf8PathBuf, NativePathErrorKind, PathAnchor};
use std::path::{Path, PathBuf};

/// A guard that restores the original cwd when dropped.
///
/// Windows can't delete a directory that is a process's cwd. Declare the guard
/// after creating the temp dir so the original cwd is restored before the temp
/// dir is removed.
struct CurrentDirGuard {
    original: PathBuf,
}

impl CurrentDirGuard {
    #[track_caller]
    fn enter(dir: &Path) -> Self {
        let original = std::env::current_dir().expect("read original current directory");
        std::env::set_current_dir(dir).expect("entered directory");
        Self { original }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        if let Err(error) = std::env::set_current_dir(&self.original) {
            eprintln!(
                "failed to restore current directory to {}: {error}",
                self.original.display()
            );
        }
    }
}

#[cfg(all(unix, not(target_vendor = "apple")))]
fn non_utf8_dir_name() -> std::ffi::OsString {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    OsStr::from_bytes(b"non-utf8-\xff").to_owned()
}

#[cfg(windows)]
fn non_utf8_dir_name() -> std::ffi::OsString {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};

    const LONE_SURROGATE: u16 = 0xD800;
    let mut wide: Vec<u16> = "non-utf8-".encode_utf16().collect();
    wide.push(LONE_SURROGATE);
    OsString::from_wide(&wide)
}

#[track_caller]
fn assert_relative_input_ignores_current_dir(dir: &Utf8Path) {
    let explicit_base =
        PathAnchor::new(AbsUtf8PathBuf::new(dir).expect("temp dir path is absolute"));
    let resolved = explicit_base
        .resolve_input("config.toml")
        .expect("relative input resolves without reading the current directory");
    assert_eq!(
        resolved.absolute().as_path(),
        explicit_base.directory().as_path().join("config.toml"),
    );
}

// Apple platforms are excluded because APFS rejects non-UTF-8 names.
#[cfg(not(target_vendor = "apple"))]
#[test]
fn non_utf8_current_dir_is_reported() {
    use camino_anchored::ResolvePathErrorKind;

    let temp = camino_tempfile::tempdir().expect("created temp dir");
    let dir_name = non_utf8_dir_name();
    let non_utf8 = temp.path().as_std_path().join(&dir_name);
    std::fs::create_dir(&non_utf8).expect("created non-UTF-8 directory");
    let _guard = CurrentDirGuard::enter(&non_utf8);

    let error = PathAnchor::current_dir().expect_err("non-UTF-8 current directory is rejected");
    match error.kind() {
        NativePathErrorKind::NonUtf8(error) => {
            assert_eq!(error.as_path().file_name(), Some(dir_name.as_os_str()));
        }
        other => panic!("expected NativePathErrorKind::NonUtf8, got {other:?}"),
    }

    let error = AbsUtf8PathBuf::resolve_against_current_dir("config.toml")
        .expect_err("non-UTF-8 current directory is rejected");
    assert_eq!(error.input().as_str(), "config.toml");
    match error.kind() {
        ResolvePathErrorKind::Native(NativePathErrorKind::NonUtf8(error)) => {
            assert!(
                error
                    .as_path()
                    .ends_with(Path::new(&dir_name).join("config.toml")),
                "{error:?}"
            );
        }
        other => panic!("expected ResolvePathErrorKind::Native(NonUtf8), got {other:?}"),
    }
    let message = error.to_string();
    assert!(
        message.starts_with("resolved `config.toml` to `"),
        "{message}"
    );
    assert!(
        message.ends_with("`, which is not valid UTF-8"),
        "{message}"
    );

    assert_relative_input_ignores_current_dir(temp.path());
}

// Windows is excluded because it keeps a handle to the current directory open,
// so it cannot be deleted while it is the cwd.
#[cfg(not(windows))]
#[test]
fn deleted_current_dir_is_reported() {
    use std::{error::Error, io};

    #[track_caller]
    fn assert_io_source_kind(error: &dyn Error, expected: io::ErrorKind) {
        let source = error
            .source()
            .expect("error has a source")
            .downcast_ref::<io::Error>()
            .expect("source is an io::Error");
        assert_eq!(source.kind(), expected);
    }

    let temp = camino_tempfile::tempdir().expect("created temp dir");
    let deleted = temp.path().join("deleted");
    std::fs::create_dir(&deleted).expect("created directory to delete");
    let _guard = CurrentDirGuard::enter(deleted.as_std_path());
    std::fs::remove_dir(&deleted).expect("deleted current directory");

    let error = PathAnchor::current_dir().expect_err("deleted current directory is rejected");
    match error.kind() {
        NativePathErrorKind::Io(_) => {
            assert_eq!(error.to_string(), "failed to read the current directory");
            assert_io_source_kind(&error, io::ErrorKind::NotFound);
        }
        other => panic!("expected NativePathErrorKind::Io, got {other:?}"),
    }

    let error = AbsUtf8PathBuf::resolve_against_current_dir("config.toml")
        .expect_err("deleted current directory is an error");
    assert_eq!(
        error.to_string(),
        "failed to resolve `config.toml` to an absolute path"
    );
    assert_io_source_kind(&error, io::ErrorKind::NotFound);

    assert_relative_input_ignores_current_dir(temp.path());
}

#[cfg(windows)]
#[test]
fn partial_windows_inputs_resolve_against_process_current_dir() {
    use camino::{Utf8Component, Utf8PathBuf, Utf8Prefix};

    let temp = camino_tempfile::tempdir().expect("created temp dir");
    let _guard = CurrentDirGuard::enter(temp.path().as_std_path());
    let current_dir =
        PathAnchor::current_dir().expect("current directory is readable, UTF-8, and absolute");
    let current_dir = current_dir.directory().as_path();

    let drive = match current_dir.components().next() {
        Some(Utf8Component::Prefix(prefix)) => match prefix.kind() {
            Utf8Prefix::Disk(letter) => char::from(letter),
            other => panic!("expected the temp dir to be on a drive letter, got {other:?}"),
        },
        other => panic!("expected the current directory to have a prefix, got {other:?}"),
    };
    let other_drive = if drive.eq_ignore_ascii_case(&'Z') {
        'Y'
    } else {
        'Z'
    };
    let base = PathAnchor::new(
        AbsUtf8PathBuf::new(format!(r"{other_drive}:\explicit-base")).expect("base is absolute"),
    );

    let drive_root_config = Utf8PathBuf::from(format!(r"{drive}:\config.toml"));
    for (input, expected) in [
        (
            format!("{drive}:config.toml"),
            current_dir.join("config.toml"),
        ),
        (format!("{drive}:"), current_dir.to_owned()),
        (r"\config.toml".to_owned(), drive_root_config.clone()),
        ("/config.toml".to_owned(), drive_root_config),
    ] {
        let resolved = base.resolve_input(&input).expect("partial input resolves");
        assert_eq!(
            resolved.absolute().as_path(),
            expected.as_path(),
            "{input:?}"
        );
        assert_eq!(
            resolved.display().to_string(),
            resolved.absolute().as_path().as_str(),
            "{input:?}"
        );
    }
}
