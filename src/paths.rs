// Copyright (c) The camino-anchored Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::errors::{
    AbsUtf8PathError, AbsUtf8PathErrorKind, CurrentDirError, MalformedPathKind,
    NativePathErrorKind, RelUtf8PathError, RelUtf8PathErrorKind, ResolvePathError,
    ResolvePathErrorKind, TryFromPathBufError,
};
use camino::{Utf8Component, Utf8Path, Utf8PathBuf, Utf8Prefix};
use std::{
    ffi::OsString,
    fmt, io,
    path::{Path, PathBuf},
};

/// An absolute UTF-8 path.
///
/// This is a newtype wrapper around [`Utf8PathBuf`] which guarantees that the
/// path is absolute and does not contain a NUL byte. The newtype does not imply
/// anything about whether the path exists on disk.
///
/// # Implementations
///
/// This type implements [`AsRef<Utf8Path>`] and [`AsRef<Path>`] for direct use
/// with APIs that accept these types. It can also be consumed into a
/// [`Utf8PathBuf`] or [`PathBuf`] through [`From`].
///
/// Equality, ordering, and hashing follow [`Utf8Path`]'s component-based
/// semantics: paths that differ only in spelling, such as `/repo//a` and
/// `/repo/a`, compare equal. [`Self::strip_prefix`] and [`Display`](fmt::Display)
/// preserve the original spelling regardless.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbsUtf8PathBuf(Utf8PathBuf);

impl AbsUtf8PathBuf {
    /// Returns a new [`AbsUtf8PathBuf`] from the given path.
    ///
    /// This does not normalize or change the spelling of the path in any way.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, AbsUtf8PathErrorKind};
    ///
    /// // A valid absolute path on Unix.
    /// #[cfg(unix)]
    /// let path = AbsUtf8PathBuf::new("/home/user").unwrap();
    ///
    /// // A valid absolute path on Windows.
    /// #[cfg(windows)]
    /// let path = AbsUtf8PathBuf::new(r"C:\Users\user").unwrap();
    ///
    /// // Relative paths are rejected.
    /// let error = AbsUtf8PathBuf::new("foo/bar").unwrap_err();
    /// assert_eq!(error.kind(), AbsUtf8PathErrorKind::NotAbsolute);
    ///
    /// // Paths containing NUL bytes are rejected.
    /// #[cfg(unix)]
    /// let error = AbsUtf8PathBuf::new("/home/user\0").unwrap_err();
    /// #[cfg(windows)]
    /// let error = AbsUtf8PathBuf::new("C:\\Users\\user\0").unwrap_err();
    /// assert_eq!(error.kind(), AbsUtf8PathErrorKind::ContainsNul);
    ///
    /// // Drive-relative and root-relative Windows paths are also rejected.
    /// #[cfg(windows)]
    /// {
    ///     let error = AbsUtf8PathBuf::new(r"C:foo").unwrap_err();
    ///     assert_eq!(error.kind(), AbsUtf8PathErrorKind::DriveRelative);
    ///     let error = AbsUtf8PathBuf::new(r"\foo").unwrap_err();
    ///     assert_eq!(error.kind(), AbsUtf8PathErrorKind::RootRelative);
    /// }
    /// ```
    pub fn new<P: Into<Utf8PathBuf>>(path: P) -> Result<Self, AbsUtf8PathError> {
        match classify_path(path.into()) {
            PathClass::Absolute(absolute) => Ok(absolute),
            PathClass::Malformed(path, kind) => Err(AbsUtf8PathError::new(path, kind.into())),
            PathClass::Relative(relative) => Err(AbsUtf8PathError::new(
                relative.into_path_buf(),
                AbsUtf8PathErrorKind::NotAbsolute,
            )),
            PathClass::RootRelative(path) => Err(AbsUtf8PathError::new(
                path,
                AbsUtf8PathErrorKind::RootRelative,
            )),
            PathClass::DriveRelative(path) => Err(AbsUtf8PathError::new(
                path,
                AbsUtf8PathErrorKind::DriveRelative,
            )),
        }
    }

    /// Resolves a path using native platform semantics, consulting the
    /// current directory for relative inputs.
    ///
    /// This is a thin wrapper around [`std::path::absolute`] — see its
    /// documentation for more information. The most relevant semantics are:
    ///
    /// * It does not resolve symlinks or require existence.
    /// * On Unix, `.` and repeated separators are removed, while `..`
    ///   and trailing separators are preserved.
    /// * On Windows, verbatim paths are returned unchanged, while non-verbatim
    ///   paths are resolved according to the platform's rules. In particular,
    ///   `..` components are collapsed lexically, without following symlinks.
    ///
    /// Returns an error if:
    ///
    /// * The input is empty or contains a NUL byte.
    /// * For a relative path, the current directory could not be read.
    /// * The resolved path is not valid UTF-8.
    /// * The OS produced an error.
    /// * The OS reported success, but the resolved path is not absolute (see
    ///   [`NativePathErrorKind::Invalid`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, ResolvePathErrorKind};
    ///
    /// // Resolve an input relative to the process current directory.
    /// let path = AbsUtf8PathBuf::resolve_against_current_dir("config.toml").unwrap();
    /// assert!(path.as_path().is_absolute());
    /// assert!(path.as_path().ends_with("config.toml"));
    ///
    /// // Empty inputs are rejected.
    /// let error = AbsUtf8PathBuf::resolve_against_current_dir("").unwrap_err();
    /// match error.kind() {
    ///     ResolvePathErrorKind::Empty => {}
    ///     other => panic!("expected Empty, got {other:?}"),
    /// }
    /// ```
    pub fn resolve_against_current_dir<P: AsRef<Utf8Path>>(
        path: P,
    ) -> Result<Self, ResolvePathError> {
        let input = path.as_ref();
        let kind = match malformed_kind(input) {
            Some(kind) => kind.into(),
            None => match Self::from_native(std::path::absolute(input)) {
                Ok(resolved) => return Ok(resolved),
                Err(kind) => ResolvePathErrorKind::Native(kind),
            },
        };
        Err(ResolvePathError::new(input.to_owned(), kind))
    }

    /// Returns the process's current directory.
    ///
    /// Returns an error if the current directory could not be read, is not
    /// valid UTF-8, or is not absolute.
    ///
    /// # Notes
    ///
    /// This uses [`std::env::current_dir`] to get the current directory.
    ///
    /// * On Unix, the current directory is the physical path returned by `getcwd`,
    ///   with symlinks resolved. This can differ from the logical path a shell
    ///   tracks in `$PWD`. For example, after `cd /home/user/link`, where `link`
    ///   is a symlink to `/data/project`, the result is `/data/project`.
    ///
    /// * On Windows, [`std::env::current_dir`] currently calls `GetCurrentDirectoryW`.
    ///   This returns the logical path without resolving symlinks or junctions.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, RelUtf8PathBuf};
    ///
    /// let current_dir = AbsUtf8PathBuf::current_dir().unwrap();
    ///
    /// // The result is the process's current directory.
    /// assert_eq!(
    ///     current_dir.as_path().as_std_path(),
    ///     std::env::current_dir().unwrap(),
    /// );
    ///
    /// // Join relative paths onto it to get absolute locations.
    /// let config = current_dir.join(&RelUtf8PathBuf::new("config.toml").unwrap());
    /// assert_eq!(config.as_path(), current_dir.as_path().join("config.toml"));
    /// ```
    pub fn current_dir() -> Result<Self, CurrentDirError> {
        Self::from_native(std::env::current_dir()).map_err(CurrentDirError::new)
    }

    /// Returns the process's logical current directory if it can be determined.
    ///
    /// Returns an error if the current directory could not be read, is not
    /// valid UTF-8, or is not absolute.
    ///
    /// # Notes
    ///
    /// If the `PWD` environment variable is set, it is checked first. It is
    /// verified to be absolute on the current platform, valid UTF-8, and to
    /// canonicalize to the same directory as the physical current directory.
    /// If any of these conditions are not met, the physical current directory
    /// is used instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, RelUtf8PathBuf};
    ///
    /// let current_dir = AbsUtf8PathBuf::logical_current_dir().unwrap();
    ///
    /// // The result names the same directory as the physical current
    /// // directory, though its spelling may differ if the directory was
    /// // entered through a symlink.
    /// assert_eq!(
    ///     std::fs::canonicalize(&current_dir).unwrap(),
    ///     std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
    /// );
    ///
    /// // Join relative paths onto it to get absolute locations.
    /// let config = current_dir.join(&RelUtf8PathBuf::new("config.toml").unwrap());
    /// assert_eq!(config.as_path(), current_dir.as_path().join("config.toml"));
    /// ```
    pub fn logical_current_dir() -> Result<Self, CurrentDirError> {
        let physical = Self::current_dir()?;
        Ok(choose_logical_current_dir(
            std::env::var_os("PWD"),
            physical,
        ))
    }

    fn from_native(path: io::Result<PathBuf>) -> Result<Self, NativePathErrorKind> {
        let path = path.map_err(NativePathErrorKind::Io)?;
        let path = Utf8PathBuf::try_from(path).map_err(NativePathErrorKind::NonUtf8)?;
        Self::new(path).map_err(NativePathErrorKind::Invalid)
    }

    /// Returns the path as a borrowed [`Utf8Path`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8Path;
    /// use camino_anchored::AbsUtf8PathBuf;
    ///
    /// let path = AbsUtf8PathBuf::current_dir().unwrap();
    /// let borrowed: &Utf8Path = path.as_path();
    /// assert!(borrowed.is_absolute());
    /// ```
    #[must_use]
    pub fn as_path(&self) -> &Utf8Path {
        &self.0
    }

    /// Returns the path as an owned [`Utf8PathBuf`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8PathBuf;
    /// use camino_anchored::AbsUtf8PathBuf;
    ///
    /// let path = AbsUtf8PathBuf::current_dir().unwrap();
    /// let mut owned: Utf8PathBuf = path.into_path_buf();
    /// owned.push("config.toml");
    /// assert!(owned.is_absolute());
    /// assert!(owned.ends_with("config.toml"));
    /// ```
    #[must_use]
    pub fn into_path_buf(self) -> Utf8PathBuf {
        self.0
    }

    /// Returns the parent directory of `self`, or `None` if `self` is a root
    /// such as `/`, `C:\`, or `\\server\share`.
    ///
    /// # Notes
    ///
    /// This is a thin wrapper around [`Utf8Path::parent`], so the result is
    /// computed from the path's text, without consulting the file system.
    ///
    /// A common use is to anchor paths relative to a file's directory, such as
    /// paths mentioned inside a config file.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, RelUtf8PathBuf};
    ///
    /// let config = AbsUtf8PathBuf::resolve_against_current_dir("project/config.toml").unwrap();
    /// let project = config.parent().unwrap();
    /// assert_eq!(project.as_path(), config.as_path().parent().unwrap());
    ///
    /// // The parent is computed without consulting the file system, so `..` is removed rather
    /// // than resolved.
    /// let path = project.join(&RelUtf8PathBuf::new("..").unwrap());
    /// assert_eq!(path.parent().unwrap(), project);
    ///
    /// // Roots have no parent.
    /// #[cfg(unix)]
    /// assert_eq!(AbsUtf8PathBuf::new("/").unwrap().parent(), None);
    /// #[cfg(windows)]
    /// assert_eq!(AbsUtf8PathBuf::new(r"C:\").unwrap().parent(), None);
    /// ```
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let parent = self.0.parent()?;
        Some(Self::new(parent.to_owned()).expect("parent of an absolute path is absolute"))
    }

    /// Resolves a relative path against `self`, interpreted as a directory.
    ///
    /// # Notes
    ///
    /// Since the suffix is relative, it cannot replace the root or Windows
    /// prefix.
    ///
    /// Parent components are allowed: this operation does not guarantee
    /// that the resulting path is contained within `self`.
    ///
    /// On Windows, if `self` is a verbatim path (for example, `\\?\C:\repo`),
    /// the result is normalized, similar to what [`PathBuf::push`] does:
    ///
    /// * `.` components are removed.
    /// * Each `..` component causes the component before it to be dropped.
    ///   (If the `..` is at the root, it is ignored.)
    /// * Separators are replaced with `\`, and trailing separators are dropped.
    ///
    /// Windows doesn't interpret `.`, `..`, or `/` in verbatim paths, so this
    /// is necessary for the result to work correctly. But it does mean that `..`
    /// is resolved lexically rather than by following symlinks.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, RelUtf8PathBuf};
    ///
    /// let directory = AbsUtf8PathBuf::current_dir().unwrap();
    /// let relative = RelUtf8PathBuf::new("config.toml").unwrap();
    /// let path = directory.join(&relative);
    /// assert_eq!(path.as_path(), directory.as_path().join("config.toml"));
    /// ```
    #[must_use]
    pub fn join(&self, suffix: &RelUtf8PathBuf) -> Self {
        Self(self.0.join(&suffix.0))
    }

    /// Strips `base`, preserving the way `self` is spelled afterwards.
    ///
    /// Returns `.` if `self` and `base` are spelled the same way, ignoring
    /// trailing separators on either side.
    ///
    /// The result may contain `..` components: for example, stripping `/repo`
    /// from `/repo/../x` returns `../x`.
    ///
    /// Returns `None` if any of the following conditions are met:
    ///
    /// - On Windows, either `self` or `base` is a verbatim or device path.
    /// - On Unix, `self` is of the form `//foo` and `base` is of the form `/bar`,
    ///   or vice versa.
    /// - [`Utf8Path::strip_prefix`] returns an error for the pair.
    /// - [`str::strip_prefix`] returns `None` for the pair (in other words,
    ///   matching is sensitive to the way paths are spelled).
    /// - After the separators between `base` and the remainder are removed,
    ///   the remainder is not a well-formed relative path (for example,
    ///   `C:stream` in `C:\repo\C:stream` on Windows).
    /// - `base`'s spelling doesn't end at a separator boundary in `self`.
    ///
    /// If the result is `Some`, it is guaranteed that `base.join(&result)`
    /// returns a path equal to `self` when compared as [`AbsUtf8PathBuf`] or
    /// [`Utf8Path`], though it may not necessarily be exactly the same when
    /// compared as strings.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, RelUtf8PathBuf};
    ///
    /// let base = AbsUtf8PathBuf::resolve_against_current_dir("project").unwrap();
    ///
    /// // The remaining path keeps its original spelling.
    /// let path = base.join(&RelUtf8PathBuf::new("config//app.toml").unwrap());
    /// let relative = path.strip_prefix(&base).unwrap();
    /// assert_eq!(relative.as_path().as_str(), "config//app.toml");
    ///
    /// // Stripping a path from itself returns `.`.
    /// let relative = base.strip_prefix(&base).unwrap();
    /// assert_eq!(relative.as_path().as_str(), ".");
    ///
    /// // Unrelated paths return `None`.
    /// let outside = AbsUtf8PathBuf::resolve_against_current_dir("other.toml").unwrap();
    /// assert_eq!(outside.strip_prefix(&base), None);
    /// ```
    #[must_use]
    pub fn strip_prefix(&self, base: &AbsUtf8PathBuf) -> Option<RelUtf8PathBuf> {
        strip_prefix_preserving_spelling(self.as_path(), base.as_path())
    }
}

impl fmt::Display for AbsUtf8PathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl AsRef<Utf8Path> for AbsUtf8PathBuf {
    /// Borrows the absolute path as a [`Utf8Path`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8Path;
    /// use camino_anchored::AbsUtf8PathBuf;
    ///
    /// let path = AbsUtf8PathBuf::current_dir().unwrap();
    /// let borrowed: &Utf8Path = path.as_ref();
    /// assert!(borrowed.is_absolute());
    /// ```
    fn as_ref(&self) -> &Utf8Path {
        self.as_path()
    }
}

impl AsRef<Path> for AbsUtf8PathBuf {
    /// Borrows the absolute path as a [`Path`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::AbsUtf8PathBuf;
    /// use std::path::Path;
    ///
    /// let path = AbsUtf8PathBuf::current_dir().unwrap();
    /// let borrowed: &Path = path.as_ref();
    /// assert!(borrowed.is_absolute());
    /// ```
    fn as_ref(&self) -> &Path {
        self.as_path().as_std_path()
    }
}

impl From<AbsUtf8PathBuf> for Utf8PathBuf {
    /// Consumes the wrapper, returning a [`Utf8PathBuf`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8PathBuf;
    /// use camino_anchored::AbsUtf8PathBuf;
    ///
    /// let path = AbsUtf8PathBuf::resolve_against_current_dir("config.toml").unwrap();
    /// let expected = path.as_path().as_str().to_owned();
    /// let owned: Utf8PathBuf = path.into();
    /// assert_eq!(owned.as_str(), expected);
    /// ```
    fn from(path: AbsUtf8PathBuf) -> Self {
        path.into_path_buf()
    }
}

impl From<AbsUtf8PathBuf> for PathBuf {
    /// Consumes the wrapper, returning a [`PathBuf`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::AbsUtf8PathBuf;
    /// use std::path::PathBuf;
    ///
    /// let path = AbsUtf8PathBuf::resolve_against_current_dir("config.toml").unwrap();
    /// let expected = path.as_path().as_str().to_owned();
    /// let owned: PathBuf = path.into();
    /// assert_eq!(owned.to_str(), Some(expected.as_str()));
    /// ```
    fn from(path: AbsUtf8PathBuf) -> Self {
        path.into_path_buf().into_std_path_buf()
    }
}

// We do not implement FromStr for AbsUtf8PathBuf so that one can't directly be
// used in clap, etc. We may want to revisit this for AbsUtf8PathBuf (though
// not RelUtf8PathBuf) in the future.

impl TryFrom<Utf8PathBuf> for AbsUtf8PathBuf {
    type Error = AbsUtf8PathError;

    fn try_from(path: Utf8PathBuf) -> Result<Self, Self::Error> {
        Self::new(path)
    }
}

impl TryFrom<PathBuf> for AbsUtf8PathBuf {
    type Error = TryFromPathBufError;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        let path = Utf8PathBuf::try_from(path).map_err(TryFromPathBufError::NonUtf8)?;
        Self::new(path).map_err(TryFromPathBufError::Invalid)
    }
}

/// A well-formed relative UTF-8 path.
///
/// Here, _well-formed_ means that the following are all true:
///
/// * [`Utf8Path::is_relative`] returns true for the path.
/// * The path is not empty.
/// * The path does not contain a NUL byte.
/// * On Windows, the path is not of the form `C:foo` (drive-relative)
///   or `\foo` or `/foo` (root-relative, without a drive letter).
///
/// Parent components are allowed in the path.
///
/// # Notes
///
/// This path is not intended to be used for user input. For processing paths
/// received as user input, use
/// [`PathAnchor::resolve_input`](crate::PathAnchor::resolve_input)
/// instead.
///
/// This type does not implement [`AsRef<Utf8Path>`] or [`AsRef<Path>`]. This is
/// deliberate to avoid filesystem APIs accidentally resolving these paths
/// against the current directory; instead, resolve these paths against an
/// explicit [`PathAnchor`](crate::PathAnchor) if possible, or use [`Self::as_path`] to
/// access the unresolved path.
///
/// Equality, ordering, and hashing follow [`Utf8Path`]'s component-based
/// semantics: paths that differ only in spelling, such as `a//b` and `a/b`,
/// compare equal. [`Display`](fmt::Display) preserves the original spelling
/// regardless.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelUtf8PathBuf(Utf8PathBuf);

impl RelUtf8PathBuf {
    /// Returns a new [`RelUtf8PathBuf`] from the given path.
    ///
    /// This does not normalize or change the spelling of the path in any way.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{RelUtf8PathBuf, RelUtf8PathErrorKind};
    ///
    /// // Parent components and the original spelling are preserved.
    /// let path = RelUtf8PathBuf::new("../config.toml").unwrap();
    /// assert_eq!(path.as_path().as_str(), "../config.toml");
    ///
    /// // Empty paths are rejected.
    /// let error = RelUtf8PathBuf::new("").unwrap_err();
    /// assert_eq!(error.kind(), RelUtf8PathErrorKind::Empty);
    ///
    /// // Paths containing NUL bytes are rejected.
    /// let error = RelUtf8PathBuf::new("foo\0bar").unwrap_err();
    /// assert_eq!(error.kind(), RelUtf8PathErrorKind::ContainsNul);
    ///
    /// // Absolute paths are rejected.
    /// #[cfg(unix)]
    /// let error = RelUtf8PathBuf::new("/home/user").unwrap_err();
    /// #[cfg(windows)]
    /// let error = RelUtf8PathBuf::new(r"C:\Users\user").unwrap_err();
    /// assert_eq!(error.kind(), RelUtf8PathErrorKind::Absolute);
    ///
    /// // Drive-relative and root-relative Windows paths are also rejected.
    /// #[cfg(windows)]
    /// {
    ///     let error = RelUtf8PathBuf::new(r"C:foo").unwrap_err();
    ///     assert_eq!(error.kind(), RelUtf8PathErrorKind::DriveRelative);
    ///     let error = RelUtf8PathBuf::new(r"\foo").unwrap_err();
    ///     assert_eq!(error.kind(), RelUtf8PathErrorKind::RootRelative);
    /// }
    /// ```
    pub fn new<P: Into<Utf8PathBuf>>(path: P) -> Result<Self, RelUtf8PathError> {
        match classify_path(path.into()) {
            PathClass::Relative(relative) => Ok(relative),
            PathClass::Malformed(path, kind) => Err(RelUtf8PathError::new(path, kind.into())),
            PathClass::Absolute(absolute) => Err(RelUtf8PathError::new(
                absolute.into_path_buf(),
                RelUtf8PathErrorKind::Absolute,
            )),
            PathClass::RootRelative(path) => Err(RelUtf8PathError::new(
                path,
                RelUtf8PathErrorKind::RootRelative,
            )),
            PathClass::DriveRelative(path) => Err(RelUtf8PathError::new(
                path,
                RelUtf8PathErrorKind::DriveRelative,
            )),
        }
    }

    /// Returns the path as a borrowed [`Utf8Path`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8Path;
    /// use camino_anchored::RelUtf8PathBuf;
    ///
    /// let path = RelUtf8PathBuf::new("./config.toml").unwrap();
    /// let borrowed: &Utf8Path = path.as_path();
    /// assert_eq!(borrowed.as_str(), "./config.toml");
    /// ```
    #[must_use]
    pub fn as_path(&self) -> &Utf8Path {
        &self.0
    }

    /// Returns the path as an owned [`Utf8PathBuf`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino::Utf8PathBuf;
    /// use camino_anchored::RelUtf8PathBuf;
    ///
    /// let path = RelUtf8PathBuf::new("../config.toml").unwrap();
    /// let owned: Utf8PathBuf = path.into_path_buf();
    /// assert_eq!(owned.as_str(), "../config.toml");
    /// ```
    #[must_use]
    pub fn into_path_buf(self) -> Utf8PathBuf {
        self.0
    }
}

impl fmt::Display for RelUtf8PathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

pub(crate) fn choose_logical_current_dir(
    pwd: Option<OsString>,
    physical: AbsUtf8PathBuf,
) -> AbsUtf8PathBuf {
    let Some(pwd) = pwd else {
        return physical;
    };
    let Ok(pwd) = Utf8PathBuf::try_from(PathBuf::from(pwd)) else {
        return physical;
    };
    let Ok(pwd) = AbsUtf8PathBuf::new(pwd) else {
        return physical;
    };
    if names_same_directory(&pwd, &physical) {
        pwd
    } else {
        physical
    }
}

#[cfg(unix)]
fn names_same_directory(a: &AbsUtf8PathBuf, b: &AbsUtf8PathBuf) -> bool {
    use std::os::unix::fs::MetadataExt;

    // Compare the device and inode numbers. This is the best possible check
    // because it also works through things like bind mounts (it also matches
    // what Go and Bash do).
    //
    // std::fs::metadata uses stat, not lstat, so it follows symlinks.
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        (Err(_), Ok(_)) | (Ok(_), Err(_)) | (Err(_), Err(_)) => false,
    }
}

#[cfg(not(unix))]
fn names_same_directory(a: &AbsUtf8PathBuf, b: &AbsUtf8PathBuf) -> bool {
    // On other platforms, canonicalize both sides.
    //
    // You might ask, should we compare the file index on Windows? Well, that's
    // roughly what canonicalize does anyway (though it does a
    // GetFinalPathNameByHandle at the end), and besides, PWD isn't really set
    // on Windows in most cases so we don't expect to hit this code path much on
    // Windows. For now we don't think it's worth pulling in a dependency on the
    // windows-sys crate.
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        (Err(_), Ok(_)) | (Ok(_), Err(_)) | (Err(_), Err(_)) => false,
    }
}

pub(crate) enum PathClass {
    Malformed(Utf8PathBuf, MalformedPathKind),
    Absolute(AbsUtf8PathBuf),
    Relative(RelUtf8PathBuf),
    RootRelative(Utf8PathBuf),
    DriveRelative(Utf8PathBuf),
}

pub(crate) fn classify_path(path: Utf8PathBuf) -> PathClass {
    if let Some(kind) = malformed_kind(&path) {
        PathClass::Malformed(path, kind)
    } else if path.is_absolute() {
        PathClass::Absolute(AbsUtf8PathBuf(path))
    } else if path.has_root() {
        PathClass::RootRelative(path)
    } else if path_prefix(&path).is_some() {
        PathClass::DriveRelative(path)
    } else {
        PathClass::Relative(RelUtf8PathBuf(path))
    }
}

fn malformed_kind(path: &Utf8Path) -> Option<MalformedPathKind> {
    if path.as_str().is_empty() {
        Some(MalformedPathKind::Empty)
    } else if path.as_str().contains('\0') {
        Some(MalformedPathKind::ContainsNul)
    } else {
        None
    }
}

fn path_prefix(path: &Utf8Path) -> Option<Utf8Prefix<'_>> {
    match path.components().next() {
        Some(Utf8Component::Prefix(prefix)) => Some(prefix.kind()),
        Some(
            Utf8Component::RootDir
            | Utf8Component::CurDir
            | Utf8Component::ParentDir
            | Utf8Component::Normal(_),
        )
        | None => None,
    }
}

/// Determines the relative display suffix for two paths.
///
/// This assumes that both inputs are already known to be absolute and that
/// `base` is a directory.
///
/// Returns `None` if the path should be displayed as absolute.
fn strip_prefix_preserving_spelling(path: &Utf8Path, base: &Utf8Path) -> Option<RelUtf8PathBuf> {
    // If either path or base is a Windows verbatim (\\?\) or device namespace
    // (\\.\) path, don't try to display relative paths. (Ordinary UNC paths are
    // fine, though.)
    if is_verbatim_or_device_path(path) || is_verbatim_or_device_path(base) {
        return None;
    }

    // Require that `base` is a component prefix of `path`
    // (Utf8Path::strip_prefix). We do this check in addition to a string-based
    // prefix check (str::strip_prefix).
    //
    // Why check for the component prefix — doesn't the string-based prefix
    // subsume it? On typical Unix and Windows platforms, yes, but this isn't
    // always guaranteed. In particular, on Cygwin:
    //
    // * "//server/share" is parsed as POSIX.
    // * "//server/share\file" is parsed as a Windows UNC path.
    //
    // The str::strip_prefix check would accept this pair, but the component
    // prefix check rejects it.
    //
    // We do not read the return value of Utf8Path::strip_prefix, because we
    // want to preserve the original spelling of `path` as much as possible.
    // Utf8Path::strip_prefix can end up normalizing away separators and
    // components at the boundary. For example, if `path` is `/repo/file/` and
    // `base` is `/repo`, then this returns `file`, not `file/`. We want to
    // store paths the way they were originally spelled as far as possible.
    path.strip_prefix(base).ok()?;

    #[cfg(unix)]
    if has_double_root(path.as_str()) != has_double_root(base.as_str()) {
        // POSIX permits exactly two leading slashes to name a distinct root.
        // Component matching treats / and // alike, so check this separately.
        return None;
    }

    // Strip the prefix from the original string. We _do_ use the return value
    // of this since it is as close to intent as possible.
    let suffix = match path.as_str().strip_prefix(base.as_str()) {
        Some(suffix) => suffix,
        // `base` may carry trailing separators that `path` lacks (e.g.,
        // `/repo/` vs `/repo`). Both name the same directory, so treat this
        // like the exact-match case.
        None if trim_trailing_separators(path.as_str())
            == trim_trailing_separators(base.as_str()) =>
        {
            ""
        }
        None => return None,
    };

    // Ensure raw stripping ends at a separator boundary. Note that Component
    // matching alone is insufficient here: `/a/.` matches `/a/..hidden`
    // component-wise, but removing the raw `/a/.` prefix would return
    // `.hidden`, which is wrong.
    if !suffix.is_empty()
        && !base.as_str().ends_with(std::path::is_separator)
        && !suffix.starts_with(std::path::is_separator)
    {
        return None;
    }

    // Remove only the separators between the base and suffix so that the
    // display is relative. (Preserve internal and trailing separators.)
    let suffix = suffix.trim_start_matches(std::path::is_separator);
    if suffix.is_empty() {
        // The base itself is displayed as the current directory.
        Some(RelUtf8PathBuf::new(".").expect("`.` is a well-formed relative path"))
    } else {
        // A suffix can acquire a different interpretation when detached from
        // its prefix — for example, on Windows, C:\repo\C:stream must not
        // become the drive-relative C:stream. Require a well-formed relative
        // path.
        match RelUtf8PathBuf::new(suffix) {
            Ok(relative) => Some(relative),
            Err(error) => match error.kind() {
                RelUtf8PathErrorKind::Absolute | RelUtf8PathErrorKind::DriveRelative => None,
                RelUtf8PathErrorKind::Empty
                | RelUtf8PathErrorKind::ContainsNul
                | RelUtf8PathErrorKind::RootRelative => {
                    // The suffix is non-empty, comes from a NUL-free path, and
                    // has had its leading separators trimmed, so we should
                    // never hit this case.
                    debug_assert!(
                        false,
                        "suffix {suffix:?} is a well-formed relative path: {error}"
                    );
                    None
                }
            },
        }
    }
}

fn trim_trailing_separators(path: &str) -> &str {
    path.trim_end_matches(std::path::is_separator)
}

#[cfg(any(windows, target_os = "cygwin"))]
fn is_verbatim_or_device_path(path: &Utf8Path) -> bool {
    match path_prefix(path) {
        Some(
            Utf8Prefix::Verbatim(_)
            | Utf8Prefix::VerbatimUNC(..)
            | Utf8Prefix::VerbatimDisk(_)
            | Utf8Prefix::DeviceNS(_),
        ) => true,
        Some(Utf8Prefix::UNC(..) | Utf8Prefix::Disk(_)) | None => false,
    }
}

#[cfg(not(any(windows, target_os = "cygwin")))]
fn is_verbatim_or_device_path(_path: &Utf8Path) -> bool {
    false
}

#[cfg(unix)]
fn has_double_root(path: &str) -> bool {
    path.starts_with("//") && !path.starts_with("///")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{absolute, assert_resolve_error};
    use std::fmt;

    #[cfg(unix)]
    const ABSOLUTE: &str = "/foo/bar";
    #[cfg(windows)]
    const ABSOLUTE: &str = r"C:\foo\bar";

    #[cfg(unix)]
    const ABSOLUTE_WITH_NUL: &str = "/a\0b";
    #[cfg(windows)]
    const ABSOLUTE_WITH_NUL: &str = "C:\\a\0b";

    #[derive(Debug, PartialEq, Eq)]
    enum PathKind {
        Absolute,
        Relative,
        Neither(RelUtf8PathErrorKind),
    }

    #[track_caller]
    fn classify(input: &str) -> PathKind {
        match (AbsUtf8PathBuf::new(input), RelUtf8PathBuf::new(input)) {
            (Ok(_), Err(relative_error)) => {
                assert_eq!(relative_error.kind(), RelUtf8PathErrorKind::Absolute);
                PathKind::Absolute
            }
            (Err(_), Ok(_)) => PathKind::Relative,
            (Err(_), Err(relative_error)) => PathKind::Neither(relative_error.kind()),
            (Ok(_), Ok(_)) => panic!("{input:?} was accepted as both absolute and relative"),
        }
    }

    #[test]
    fn classify_portable() {
        assert_eq!(classify(""), PathKind::Neither(RelUtf8PathErrorKind::Empty));
        assert_eq!(
            classify("a\0b"),
            PathKind::Neither(RelUtf8PathErrorKind::ContainsNul)
        );
        assert_eq!(classify("."), PathKind::Relative);
        assert_eq!(classify(".."), PathKind::Relative);
        assert_eq!(classify("../config.toml"), PathKind::Relative);
        assert_eq!(classify("./a/../b/"), PathKind::Relative);
        assert_eq!(classify("a//b"), PathKind::Relative);
        assert_eq!(
            classify(ABSOLUTE_WITH_NUL),
            PathKind::Neither(RelUtf8PathErrorKind::ContainsNul)
        );
    }

    #[cfg(windows)]
    #[test]
    fn classify_windows() {
        assert_eq!(classify(r"C:\"), PathKind::Absolute);
        assert_eq!(classify(r"C:\a\..\b\"), PathKind::Absolute);
        assert_eq!(classify(r"\\server\share\a"), PathKind::Absolute);
        assert_eq!(classify(r"\\?\C:\a"), PathKind::Absolute);

        for input in [r"C:foo", r"C:"] {
            assert_eq!(
                classify(input),
                PathKind::Neither(RelUtf8PathErrorKind::DriveRelative),
                "{input:?}"
            );
        }
        for input in [r"\foo", "/foo"] {
            assert_eq!(
                classify(input),
                PathKind::Neither(RelUtf8PathErrorKind::RootRelative),
                "{input:?}"
            );
        }
    }

    #[test]
    fn absolute_path_error_kinds() {
        let mut cases = vec![
            ("", AbsUtf8PathErrorKind::Empty),
            ("\0", AbsUtf8PathErrorKind::ContainsNul),
            (ABSOLUTE_WITH_NUL, AbsUtf8PathErrorKind::ContainsNul),
            ("foo/bar", AbsUtf8PathErrorKind::NotAbsolute),
        ];
        if cfg!(windows) {
            cases.push((r"\foo", AbsUtf8PathErrorKind::RootRelative));
            cases.push(("/foo", AbsUtf8PathErrorKind::RootRelative));
            cases.push(("C:foo", AbsUtf8PathErrorKind::DriveRelative));
            cases.push(("C:", AbsUtf8PathErrorKind::DriveRelative));
        }
        for (input, expected) in cases {
            let error = AbsUtf8PathBuf::new(input).expect_err("path is rejected");
            assert_eq!(error.path().as_str(), input);
            assert_eq!(error.kind(), expected, "{input:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn join_onto_verbatim_base_normalizes() {
        use crate::test_helpers::relative;

        for (base, suffix, expected) in [
            (r"\\?\C:\repo", "a/b", r"\\?\C:\repo\a\b"),
            (r"\\?\C:\repo", r"a\.\b\", r"\\?\C:\repo\a\b"),
            (r"\\?\C:\repo", r"..\config.toml", r"\\?\C:\config.toml"),
            (
                r"\\?\C:\repo",
                r"link\..\config.toml",
                r"\\?\C:\repo\config.toml",
            ),
            (r"\\?\C:\", r"..\..\config.toml", r"\\?\C:\config.toml"),
        ] {
            let base = absolute(base);
            let suffix = relative(suffix);
            assert_eq!(
                base.join(&suffix).as_path().as_str(),
                expected,
                "joining {suffix:?} onto {base:?}"
            );
        }
    }

    #[test]
    fn parent_stays_absolute() {
        #[cfg(unix)]
        let cases = [
            ("/", None),
            ("/repo", Some("/")),
            ("/repo/config.toml", Some("/repo")),
            ("/repo/", Some("/")),
            ("/repo/..", Some("/repo")),
            ("//host/a", Some("//host")),
        ];
        #[cfg(windows)]
        let cases = [
            (r"C:\", None),
            (r"C:\repo", Some(r"C:\")),
            (r"C:\repo\config.toml", Some(r"C:\repo")),
            (r"\\server\share", None),
            (r"\\server\share\a", Some(r"\\server\share\")),
            (r"\\?\C:\", None),
            (r"\\?\C:\a", Some(r"\\?\C:\")),
        ];
        for (input, expected) in cases {
            assert_eq!(
                absolute(input)
                    .parent()
                    .as_ref()
                    .map(|parent| parent.as_path().as_str()),
                expected,
                "{input:?}"
            );
        }
    }

    #[test]
    fn display_preserves_spelling() {
        use crate::test_helpers::relative;

        let spelled = format!("{ABSOLUTE}//x/./");
        assert_eq!(absolute(spelled.as_str()).to_string(), spelled);
        assert_eq!(relative("a//b/./").to_string(), "a//b/./");
        assert_eq!(format!("{:>10}", relative("a//b/./")), "   a//b/./");
    }

    #[test]
    fn logical_current_dir_falls_back_to_physical() {
        let temp = camino_tempfile::tempdir().expect("created temp dir");
        let physical = absolute(temp.path());
        let mut cases = vec![
            None,
            Some(OsString::from("relative")),
            Some(temp.path().join("missing").into_os_string()),
        ];
        if cfg!(windows) {
            // Cygwin and MSYS2 pass `PWD` to native children in POSIX form,
            // which is root-relative on Windows and must not be mistaken for
            // the current directory.
            cases.push(Some(OsString::from("/cygdrive/c/Users/me")));
            cases.push(Some(OsString::from("/c/Users/me")));
            cases.push(Some(OsString::from("/home/me")));
        }
        for pwd in cases {
            assert_eq!(
                choose_logical_current_dir(pwd.clone(), physical.clone()),
                physical,
                "{pwd:?}"
            );
        }
        assert_eq!(
            choose_logical_current_dir(Some(temp.path().as_os_str().to_owned()), physical.clone()),
            physical,
        );
    }

    #[cfg(unix)]
    #[test]
    fn logical_current_dir_prefers_pwd_through_symlink() {
        use std::{
            ffi::OsStr,
            os::unix::{ffi::OsStrExt, fs::symlink},
        };

        let temp = camino_tempfile::tempdir().expect("created temp dir");
        let target = temp.path().join("target");
        let link = temp.path().join("link");
        std::fs::create_dir(&target).expect("created target");
        symlink(&target, &link).expect("created link");
        let physical = absolute(&target);

        assert_eq!(
            choose_logical_current_dir(Some(link.as_os_str().to_owned()), physical.clone()),
            absolute(&link),
        );
        assert_eq!(
            choose_logical_current_dir(Some(temp.path().as_os_str().to_owned()), physical.clone()),
            physical,
        );
        assert_eq!(
            choose_logical_current_dir(
                Some(OsStr::from_bytes(b"/\xff").to_owned()),
                physical.clone()
            ),
            physical,
        );
    }

    #[test]
    fn resolve_against_current_dir_rejects_invalid_input() {
        for (input, expected) in [
            ("", ResolvePathErrorKind::Empty),
            ("a\0b", ResolvePathErrorKind::ContainsNul),
            ("\0", ResolvePathErrorKind::ContainsNul),
        ] {
            assert_resolve_error(
                AbsUtf8PathBuf::resolve_against_current_dir(input),
                input,
                expected,
            );
        }
    }

    #[test]
    fn absolute_path_conversions_match_new() {
        for input in ["", ".", "foo/../bar", "a\0b", ABSOLUTE] {
            assert_eq!(
                AbsUtf8PathBuf::try_from(Utf8PathBuf::from(input)),
                AbsUtf8PathBuf::new(input),
                "{input:?}"
            );
            assert_eq!(
                AbsUtf8PathBuf::try_from(PathBuf::from(input)),
                AbsUtf8PathBuf::new(input).map_err(TryFromPathBufError::Invalid),
                "{input:?}"
            );
        }
    }

    #[track_caller]
    fn assert_non_utf8<T: fmt::Debug>(result: Result<T, TryFromPathBufError>, expected: &Path) {
        match result {
            Err(TryFromPathBufError::NonUtf8(error)) => {
                assert_eq!(error.into_path_buf(), expected);
            }
            other => panic!("expected NonUtf8 for {expected:?}, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn try_from_path_buf_rejects_non_utf8() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

        let absolute = PathBuf::from(OsStr::from_bytes(b"/foo/\xff"));
        assert_non_utf8(AbsUtf8PathBuf::try_from(absolute.clone()), &absolute);
    }

    #[cfg(windows)]
    #[test]
    fn try_from_path_buf_rejects_non_utf8() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};

        const LONE_SURROGATE: u16 = 0xD800;
        let absolute = PathBuf::from(OsString::from_wide(&[
            u16::from(b'C'),
            u16::from(b':'),
            u16::from(b'\\'),
            LONE_SURROGATE,
        ]));
        assert_non_utf8(AbsUtf8PathBuf::try_from(absolute.clone()), &absolute);
    }

    #[track_caller]
    fn assert_strip_prefix(base: &str, path: &str, expected: Option<&str>) {
        let base = absolute(base);
        let path = absolute(path);
        let stripped = path.strip_prefix(&base);
        assert_eq!(
            stripped
                .as_ref()
                .map(|relative| relative.as_path().as_str()),
            expected,
            "stripping {base:?} from {path:?}"
        );
        if let Some(stripped) = stripped {
            assert_eq!(
                base.join(&stripped).as_path(),
                path.as_path(),
                "joining {stripped:?} onto {base:?}"
            );
        }
    }

    #[test]
    fn strip_prefix_portable() {
        #[cfg(unix)]
        let (base, separator) = ("/repo", "/");
        #[cfg(windows)]
        let (base, separator) = (r"C:\repo", r"\");
        let parent = Utf8Path::new(base).parent().expect("base has a parent");

        assert_strip_prefix(base, base, Some("."));
        assert_strip_prefix(base, &format!("{base}{separator}"), Some("."));
        assert_strip_prefix(base, &format!("{base}{separator}{separator}"), Some("."));
        assert_strip_prefix(&format!("{base}{separator}"), base, Some("."));
        assert_strip_prefix(
            &format!("{base}{separator}{separator}"),
            &format!("{base}{separator}"),
            Some("."),
        );
        assert_strip_prefix(&format!("{base}{separator}"), &format!("{base}itory"), None);
        assert_strip_prefix(
            base,
            &format!("{base}{separator}{separator}config.toml"),
            Some("config.toml"),
        );
        for suffix in [
            "config.toml",
            "a/../b/",
            "a//b",
            "file/",
            "file/.",
            "file//",
            "file/./",
        ] {
            assert_strip_prefix(base, &format!("{base}{separator}{suffix}"), Some(suffix));
        }
        assert_strip_prefix(base, &format!("{base}itory{separator}file"), None);
        assert_strip_prefix(base, parent.join("other.toml").as_str(), None);
        assert_strip_prefix(base, parent.as_str(), None);
    }

    #[cfg(unix)]
    #[test]
    fn strip_prefix_unix() {
        for (base, path, expected) in [
            ("/repo/", "/repo/config.toml", Some("config.toml")),
            ("/repo/", "/repo", Some(".")),
            ("/repo/.", "/repo/config.toml", None),
            ("/a/./", "/a/b", None),
            ("/", "/config.toml", Some("config.toml")),
            ("/", "///a", Some("a")),
            ("/", "//host/config.toml", None),
            ("//host", "/host/config.toml", None),
            ("//host", "//host/config.toml", Some("config.toml")),
            ("//", "///a", None),
            ("///", "//a", None),
            ("/a/.", "/a/.config", None),
            ("/a/.", "/a/../file", None),
            ("/a/.", "/a/..hidden", None),
        ] {
            assert_strip_prefix(base, path, expected);
        }
    }

    #[cfg(windows)]
    #[test]
    fn strip_prefix_windows() {
        for (base, path, expected) in [
            (r"C:\repo", r"C:\repo\C:stream", None),
            (r"C:\repo", r"D:\repo\config.toml", None),
            (r"C:\repo", r"\\server\share\config.toml", None),
            (r"C:\repo", r"c:\repo\config.toml", None),
            (r"C:\Repo", r"C:\repo\config.toml", None),
            (r"\\?\C:\repo", r"\\?\C:\repo\file.", None),
            (
                r"\\?\UNC\server\share\repo",
                r"\\?\UNC\server\share\repo\file.",
                None,
            ),
            (r"\\.\C:\repo", r"\\.\C:\repo\file.", None),
        ] {
            assert_strip_prefix(base, path, expected);
        }
    }
}
