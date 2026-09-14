// Copyright (c) The camino-anchored Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use camino::{FromPathBufError, Utf8Path, Utf8PathBuf};
use std::{error::Error, fmt, io};

/// An error indicating that a path was not a well-formed absolute path.
///
/// Returned by [`AbsUtf8PathBuf::new`](crate::AbsUtf8PathBuf::new).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbsUtf8PathError {
    /// The path that was rejected.
    path: Utf8PathBuf,

    /// The reason the path was rejected.
    kind: AbsUtf8PathErrorKind,
}

impl AbsUtf8PathError {
    pub(crate) fn new(path: Utf8PathBuf, kind: AbsUtf8PathErrorKind) -> Self {
        Self { path, kind }
    }

    /// Returns the path that was rejected.
    #[must_use]
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// Consumes the error, returning the path that was rejected.
    #[must_use]
    pub fn into_path(self) -> Utf8PathBuf {
        self.path
    }

    /// Returns the reason the path was rejected.
    #[must_use]
    pub fn kind(&self) -> AbsUtf8PathErrorKind {
        self.kind
    }
}

impl fmt::Display for AbsUtf8PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = &self.path;
        match self.kind {
            AbsUtf8PathErrorKind::Empty => {
                write!(f, "expected an absolute path, but the path is empty")
            }
            AbsUtf8PathErrorKind::ContainsNul => {
                write!(
                    f,
                    "expected an absolute path, but {path:?} contains a NUL byte"
                )
            }
            AbsUtf8PathErrorKind::NotAbsolute => {
                write!(f, "expected an absolute path, got `{path}`")
            }
            AbsUtf8PathErrorKind::RootRelative => write!(
                f,
                "expected an absolute path, but `{path}` is relative to the root \
                 of the current drive"
            ),
            AbsUtf8PathErrorKind::DriveRelative => write!(
                f,
                "expected an absolute path, but `{path}` is relative to the \
                 current directory of its drive"
            ),
        }
    }
}

impl Error for AbsUtf8PathError {}

/// The reason a path is not a well-formed absolute path.
///
/// Part of [`AbsUtf8PathError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AbsUtf8PathErrorKind {
    /// The path is empty.
    Empty,

    /// The path contains a NUL byte.
    ///
    /// No supported platform allows NUL bytes in paths.
    ContainsNul,

    /// The path is not absolute, e.g. `foo/bar`.
    NotAbsolute,

    /// The path is root-relative, e.g. `\foo` on Windows.
    RootRelative,

    /// The path is drive-relative, e.g. `C:foo` or `C:` on Windows.
    DriveRelative,
}

/// An error indicating that a path was not a well-formed relative path.
///
/// Returned by [`RelUtf8PathBuf::new`](crate::RelUtf8PathBuf::new).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelUtf8PathError {
    /// The non-well-formed relative path.
    path: Utf8PathBuf,

    /// The reason the path was rejected.
    kind: RelUtf8PathErrorKind,
}

impl RelUtf8PathError {
    pub(crate) fn new(path: Utf8PathBuf, kind: RelUtf8PathErrorKind) -> Self {
        Self { path, kind }
    }

    /// Returns the path that was rejected.
    #[must_use]
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// Consumes the error, returning the path that was rejected.
    #[must_use]
    pub fn into_path(self) -> Utf8PathBuf {
        self.path
    }

    /// Returns the reason the path was rejected.
    #[must_use]
    pub fn kind(&self) -> RelUtf8PathErrorKind {
        self.kind
    }
}

impl fmt::Display for RelUtf8PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = &self.path;
        match self.kind {
            RelUtf8PathErrorKind::Empty => {
                write!(f, "expected a relative path, but the path is empty")
            }
            RelUtf8PathErrorKind::ContainsNul => {
                write!(
                    f,
                    "expected a relative path, but {path:?} contains a NUL byte"
                )
            }
            RelUtf8PathErrorKind::Absolute => {
                write!(f, "expected a relative path, but `{path}` is absolute")
            }
            RelUtf8PathErrorKind::RootRelative => write!(
                f,
                "expected a relative path, but `{path}` is relative to the root \
                 of the current drive (remove the leading separator to make it \
                 relative)"
            ),
            RelUtf8PathErrorKind::DriveRelative => write!(
                f,
                "expected a relative path, but `{path}` is relative to the \
                 current directory of its drive (remove the drive prefix to \
                 make it relative)"
            ),
        }
    }
}

impl Error for RelUtf8PathError {}

/// The reason a path is not a well-formed relative path.
///
/// Part of [`RelUtf8PathError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RelUtf8PathErrorKind {
    /// The path is empty.
    Empty,

    /// The path contains a NUL byte.
    ///
    /// No supported platform allows NUL bytes in paths.
    ContainsNul,

    /// The path is absolute, e.g. `/foo` on Unix or `C:\foo` on Windows.
    Absolute,

    /// The path is root-relative, e.g. `\foo` on Windows.
    RootRelative,

    /// The path is drive-relative, e.g. `C:foo` or `C:` on Windows.
    DriveRelative,
}

/// An error that occurs if an input path could not be resolved.
///
/// Returned by
/// [`AbsUtf8PathBuf::resolve_against_current_dir`](crate::AbsUtf8PathBuf::resolve_against_current_dir)
/// and [`PathAnchor::resolve_input`](crate::PathAnchor::resolve_input).
#[derive(Debug)]
pub struct ResolvePathError {
    input: Utf8PathBuf,
    kind: ResolvePathErrorKind,
}

impl ResolvePathError {
    pub(crate) fn new(input: Utf8PathBuf, kind: ResolvePathErrorKind) -> Self {
        Self { input, kind }
    }

    /// The input path that could not be resolved.
    #[must_use]
    pub fn input(&self) -> &Utf8Path {
        &self.input
    }

    /// Consumes the error, returning the input path that could not be resolved.
    #[must_use]
    pub fn into_input(self) -> Utf8PathBuf {
        self.input
    }

    /// Returns the reason resolution failed.
    #[must_use]
    pub fn kind(&self) -> &ResolvePathErrorKind {
        &self.kind
    }
}

impl fmt::Display for ResolvePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let input = &self.input;
        match &self.kind {
            ResolvePathErrorKind::Empty => write!(f, "cannot resolve an empty path"),
            ResolvePathErrorKind::ContainsNul => {
                write!(f, "cannot resolve {input:?}: path contains a NUL byte")
            }
            ResolvePathErrorKind::Native(NativePathErrorKind::Io(_)) => {
                write!(f, "failed to resolve `{input}` to an absolute path")
            }
            ResolvePathErrorKind::Native(NativePathErrorKind::NonUtf8(error)) => write!(
                f,
                "resolved `{input}` to `{}`, which is not valid UTF-8",
                error.as_path().display()
            ),
            ResolvePathErrorKind::Native(NativePathErrorKind::Invalid(error)) => {
                match error.kind() {
                    AbsUtf8PathErrorKind::Empty | AbsUtf8PathErrorKind::ContainsNul => {
                        write!(f, "resolved `{input}` to an invalid path")
                    }
                    AbsUtf8PathErrorKind::NotAbsolute
                    | AbsUtf8PathErrorKind::RootRelative
                    | AbsUtf8PathErrorKind::DriveRelative => write!(
                        f,
                        "resolved `{input}` to an invalid path \
                         (the current directory may be unreachable)"
                    ),
                }
            }
        }
    }
}

impl Error for ResolvePathError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            ResolvePathErrorKind::Empty | ResolvePathErrorKind::ContainsNul => None,
            ResolvePathErrorKind::Native(kind) => kind.source(),
        }
    }
}

/// The reason an input path could not be resolved.
///
/// Part of [`ResolvePathError`].
#[derive(Debug)]
#[non_exhaustive]
pub enum ResolvePathErrorKind {
    /// The input path is empty.
    Empty,

    /// The input path contains a NUL byte.
    ContainsNul,

    /// Native resolution failed, or its result could not be used.
    Native(NativePathErrorKind),
}

/// An error that occurred while obtaining the current directory.
///
/// Returned by
/// [`AbsUtf8PathBuf::current_dir`](crate::AbsUtf8PathBuf::current_dir) and
/// [`PathAnchor::current_dir`](crate::PathAnchor::current_dir).
#[derive(Debug)]
pub struct CurrentDirError {
    kind: NativePathErrorKind,
}

impl CurrentDirError {
    pub(crate) fn new(kind: NativePathErrorKind) -> Self {
        Self { kind }
    }

    /// Returns information about the kind of error that occurred.
    #[must_use]
    pub fn kind(&self) -> &NativePathErrorKind {
        &self.kind
    }
}

impl fmt::Display for CurrentDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            NativePathErrorKind::Io(_) => write!(f, "failed to read the current directory"),
            NativePathErrorKind::NonUtf8(error) => write!(
                f,
                "current directory `{}` is not valid UTF-8",
                error.as_path().display()
            ),
            NativePathErrorKind::Invalid(error) => match error.kind() {
                AbsUtf8PathErrorKind::Empty | AbsUtf8PathErrorKind::ContainsNul => {
                    write!(f, "current directory is invalid")
                }
                AbsUtf8PathErrorKind::NotAbsolute
                | AbsUtf8PathErrorKind::RootRelative
                | AbsUtf8PathErrorKind::DriveRelative => {
                    write!(f, "current directory is invalid (it may be unreachable)")
                }
            },
        }
    }
}

impl Error for CurrentDirError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.kind.source()
    }
}

/// A platform-level error that occurred while resolving a path.
///
/// Part of [`CurrentDirError`] and [`ResolvePathErrorKind::Native`].
#[derive(Debug)]
#[non_exhaustive]
pub enum NativePathErrorKind {
    /// A platform-level resolution failure occurred (e.g. the current directory
    /// is not readable).
    Io(io::Error),

    /// The resolved path is not valid UTF-8.
    NonUtf8(FromPathBufError),

    /// The resolved path is not a well-formed absolute path.
    ///
    /// This should not occur in typical use. On Linux systems with glibc older
    /// than 2.27, this can happen if the current directory is outside the
    /// process's root directory (for example, after a `chroot` operation):
    /// `getcwd` returns a path starting with `(unreachable)`. See
    /// [glibc bug 22679](https://sourceware.org/bugzilla/show_bug.cgi?id=22679).
    Invalid(AbsUtf8PathError),
}

impl NativePathErrorKind {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            NativePathErrorKind::Io(error) => Some(error),
            NativePathErrorKind::Invalid(error) => Some(error),
            NativePathErrorKind::NonUtf8(error) => Some(error),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum MalformedPathKind {
    Empty,
    ContainsNul,
}

impl From<MalformedPathKind> for AbsUtf8PathErrorKind {
    fn from(kind: MalformedPathKind) -> Self {
        match kind {
            MalformedPathKind::Empty => Self::Empty,
            MalformedPathKind::ContainsNul => Self::ContainsNul,
        }
    }
}

impl From<MalformedPathKind> for RelUtf8PathErrorKind {
    fn from(kind: MalformedPathKind) -> Self {
        match kind {
            MalformedPathKind::Empty => Self::Empty,
            MalformedPathKind::ContainsNul => Self::ContainsNul,
        }
    }
}

impl From<MalformedPathKind> for ResolvePathErrorKind {
    fn from(kind: MalformedPathKind) -> Self {
        match kind {
            MalformedPathKind::Empty => Self::Empty,
            MalformedPathKind::ContainsNul => Self::ContainsNul,
        }
    }
}

/// An error converting a [`PathBuf`](std::path::PathBuf) to an
/// [`AbsUtf8PathBuf`](crate::AbsUtf8PathBuf).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TryFromPathBufError {
    /// The path was not valid UTF-8.
    NonUtf8(FromPathBufError),

    /// The path was UTF-8, but was rejected by
    /// [`AbsUtf8PathBuf::new`](crate::AbsUtf8PathBuf::new).
    Invalid(AbsUtf8PathError),
}

impl fmt::Display for TryFromPathBufError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TryFromPathBufError::NonUtf8(error) => fmt::Display::fmt(error, f),
            TryFromPathBufError::Invalid(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl Error for TryFromPathBufError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TryFromPathBufError::NonUtf8(error) => error.source(),
            TryFromPathBufError::Invalid(error) => error.source(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn non_utf8_error(prefix: &str) -> FromPathBufError {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::PathBuf};

        let mut bytes = prefix.as_bytes().to_vec();
        bytes.push(0xff);
        Utf8PathBuf::try_from(PathBuf::from(OsStr::from_bytes(&bytes)))
            .expect_err("path with an invalid byte is not UTF-8")
    }

    #[cfg(windows)]
    fn non_utf8_error(prefix: &str) -> FromPathBufError {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};

        const LONE_SURROGATE: u16 = 0xD800;
        let mut wide: Vec<u16> = prefix.encode_utf16().collect();
        wide.push(LONE_SURROGATE);
        Utf8PathBuf::try_from(PathBuf::from(OsString::from_wide(&wide)))
            .expect_err("path with a lone surrogate is not UTF-8")
    }

    #[test]
    fn absolute_path_error_display() {
        for (path, kind, expected) in [
            (
                "",
                AbsUtf8PathErrorKind::Empty,
                "expected an absolute path, but the path is empty",
            ),
            (
                "/a\0b",
                AbsUtf8PathErrorKind::ContainsNul,
                r#"expected an absolute path, but "/a\0b" contains a NUL byte"#,
            ),
            (
                "foo/bar",
                AbsUtf8PathErrorKind::NotAbsolute,
                "expected an absolute path, got `foo/bar`",
            ),
            (
                r"\foo",
                AbsUtf8PathErrorKind::RootRelative,
                r"expected an absolute path, but `\foo` is relative to the root of the current drive",
            ),
            (
                "C:foo",
                AbsUtf8PathErrorKind::DriveRelative,
                "expected an absolute path, but `C:foo` is relative to the current directory of its drive",
            ),
        ] {
            let error = AbsUtf8PathError::new(Utf8PathBuf::from(path), kind);
            assert_eq!(error.to_string(), expected, "{kind:?}");
            assert!(error.source().is_none(), "{kind:?}");
        }
    }

    #[test]
    fn relative_path_error_display() {
        for (path, kind, expected) in [
            (
                "",
                RelUtf8PathErrorKind::Empty,
                "expected a relative path, but the path is empty",
            ),
            (
                "a\0b",
                RelUtf8PathErrorKind::ContainsNul,
                r#"expected a relative path, but "a\0b" contains a NUL byte"#,
            ),
            (
                "/foo",
                RelUtf8PathErrorKind::Absolute,
                "expected a relative path, but `/foo` is absolute",
            ),
            (
                r"\foo",
                RelUtf8PathErrorKind::RootRelative,
                r"expected a relative path, but `\foo` is relative to the root of the current drive (remove the leading separator to make it relative)",
            ),
            (
                "C:foo",
                RelUtf8PathErrorKind::DriveRelative,
                "expected a relative path, but `C:foo` is relative to the current directory of its drive (remove the drive prefix to make it relative)",
            ),
        ] {
            let error = RelUtf8PathError::new(Utf8PathBuf::from(path), kind);
            assert_eq!(error.to_string(), expected, "{kind:?}");
            assert!(error.source().is_none(), "{kind:?}");
        }
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn resolve_path_error_display() {
        let not_found = io::Error::from(io::ErrorKind::NotFound).to_string();
        for (input, kind, expected, expected_source) in [
            (
                "",
                ResolvePathErrorKind::Empty,
                "cannot resolve an empty path",
                None,
            ),
            (
                "a\0b",
                ResolvePathErrorKind::ContainsNul,
                r#"cannot resolve "a\0b": path contains a NUL byte"#,
                None,
            ),
            (
                "config.toml",
                ResolvePathErrorKind::Native(NativePathErrorKind::Io(io::Error::from(
                    io::ErrorKind::NotFound,
                ))),
                "failed to resolve `config.toml` to an absolute path",
                Some(not_found.as_str()),
            ),
            (
                "config.toml",
                ResolvePathErrorKind::Native(NativePathErrorKind::NonUtf8(non_utf8_error(
                    "/repo/config.toml",
                ))),
                "resolved `config.toml` to `/repo/config.toml\u{fffd}`, which is not valid UTF-8",
                Some("PathBuf contains invalid UTF-8: /repo/config.toml\u{fffd}"),
            ),
            (
                "config.toml",
                native_invalid(
                    "(unreachable)/repo/config.toml",
                    AbsUtf8PathErrorKind::NotAbsolute,
                ),
                "resolved `config.toml` to an invalid path (the current directory may be unreachable)",
                Some("expected an absolute path, got `(unreachable)/repo/config.toml`"),
            ),
            (
                "config.toml",
                native_invalid("", AbsUtf8PathErrorKind::Empty),
                "resolved `config.toml` to an invalid path",
                Some("expected an absolute path, but the path is empty"),
            ),
            (
                "config.toml",
                native_invalid("/repo/a\0b", AbsUtf8PathErrorKind::ContainsNul),
                "resolved `config.toml` to an invalid path",
                Some(r#"expected an absolute path, but "/repo/a\0b" contains a NUL byte"#),
            ),
        ] {
            let error = ResolvePathError::new(Utf8PathBuf::from(input), kind);
            assert_eq!(error.to_string(), expected, "{error:?}");
            assert_eq!(
                error.source().map(|source| source.to_string()).as_deref(),
                expected_source,
                "{error:?}"
            );
        }
    }

    fn native_invalid(path: &str, kind: AbsUtf8PathErrorKind) -> ResolvePathErrorKind {
        ResolvePathErrorKind::Native(NativePathErrorKind::Invalid(AbsUtf8PathError::new(
            Utf8PathBuf::from(path),
            kind,
        )))
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn current_dir_error_display() {
        let invalid = |path: &str, kind| {
            CurrentDirError::new(NativePathErrorKind::Invalid(AbsUtf8PathError::new(
                Utf8PathBuf::from(path),
                kind,
            )))
        };
        let not_found = io::Error::from(io::ErrorKind::NotFound).to_string();
        for (error, expected, expected_source) in [
            (
                CurrentDirError::new(NativePathErrorKind::Io(io::Error::from(
                    io::ErrorKind::NotFound,
                ))),
                "failed to read the current directory",
                Some(not_found.as_str()),
            ),
            (
                CurrentDirError::new(NativePathErrorKind::NonUtf8(non_utf8_error("/repo/"))),
                "current directory `/repo/\u{fffd}` is not valid UTF-8",
                Some("PathBuf contains invalid UTF-8: /repo/\u{fffd}"),
            ),
            (
                invalid("(unreachable)/repo", AbsUtf8PathErrorKind::NotAbsolute),
                "current directory is invalid (it may be unreachable)",
                Some("expected an absolute path, got `(unreachable)/repo`"),
            ),
            (
                invalid(r"\repo", AbsUtf8PathErrorKind::RootRelative),
                "current directory is invalid (it may be unreachable)",
                Some(
                    r"expected an absolute path, but `\repo` is relative to the root of the current drive",
                ),
            ),
            (
                invalid("", AbsUtf8PathErrorKind::Empty),
                "current directory is invalid",
                Some("expected an absolute path, but the path is empty"),
            ),
            (
                invalid("/a\0b", AbsUtf8PathErrorKind::ContainsNul),
                "current directory is invalid",
                Some(r#"expected an absolute path, but "/a\0b" contains a NUL byte"#),
            ),
        ] {
            assert_eq!(error.to_string(), expected, "{error:?}");
            assert_eq!(
                error.source().map(|source| source.to_string()).as_deref(),
                expected_source,
                "{error:?}"
            );
        }
    }
}
