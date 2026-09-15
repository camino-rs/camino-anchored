// Copyright (c) The camino-anchored Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{
    AbsUtf8PathBuf, CurrentDirError, RelUtf8PathBuf, ResolvePathError,
    paths::{PathClass, classify_path},
};
use camino::Utf8Path;
use std::fmt;

/// An explicit absolute directory used to resolve and display paths.
///
/// The base path is treated as a directory, but this fact is not checked
/// against the filesystem.
///
/// # Resolving paths
///
/// Choose a resolution method based on what you know about the input:
///
/// | method | input | use case | consults process state? |
/// | --- | --- | --- | --- |
/// | [`resolve_relative`](Self::resolve_relative) | [`RelUtf8PathBuf`] | interpret a known relative path against this base, using the same base for location and display | no |
/// | [`resolve_absolute`](Self::resolve_absolute) | [`AbsUtf8PathBuf`] | keep an existing absolute location, but allow displaying it relative to this base | no |
/// | [`resolve_input`](Self::resolve_input) | [`AsRef<Utf8Path>`] | accept arbitrary input, such as a command-line argument, and automatically choose resolution behavior | yes, on Windows and Cygwin, for drive-relative (`C:foo`) or root-relative (`\foo`) inputs; otherwise, no |
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathAnchor {
    directory: AbsUtf8PathBuf,
}

impl PathAnchor {
    /// Creates a new `PathAnchor` with the given (absolute) directory.
    ///
    /// Interprets the path as a directory without accessing the filesystem.
    /// The path need not exist, and its file type is not checked.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor};
    ///
    /// // Store the invocation directory for subsequent path resolution.
    /// let directory = AbsUtf8PathBuf::current_dir().unwrap();
    /// let base = PathAnchor::new(directory.clone());
    /// assert_eq!(base.directory(), &directory);
    /// ```
    #[must_use]
    pub fn new(directory: AbsUtf8PathBuf) -> Self {
        Self { directory }
    }

    /// Creates a new `PathAnchor` at the process's current directory.
    ///
    /// Returns an error if the current directory could not be read, is not
    /// valid UTF-8, or is not absolute.
    ///
    /// # Notes
    ///
    /// This uses [`AbsUtf8PathBuf::current_dir`], which in turn uses
    /// [`std::env::current_dir`] to get the current directory.
    ///
    /// * On Unix, the current directory is the physical path returned by `getcwd`,
    ///   with symlinks resolved. This can differ from the logical path a shell
    ///   tracks in `$PWD`. For example, after `cd /home/user/link`, where `link`
    ///   is a symlink to `/data/project`, the base is `/data/project`.
    ///
    /// * On Windows, [`std::env::current_dir`] currently calls `GetCurrentDirectoryW`.
    ///   This returns the logical path without resolving symlinks or junctions.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::PathAnchor;
    ///
    /// let base = PathAnchor::current_dir().unwrap();
    ///
    /// // The base is the process's current directory.
    /// assert_eq!(
    ///     base.directory().as_path().as_std_path(),
    ///     std::env::current_dir().unwrap(),
    /// );
    ///
    /// // Relative inputs are resolved against it and displayed as written.
    /// let path = base.resolve_input("config.toml").unwrap();
    /// assert_eq!(path.display().to_string(), "config.toml");
    /// ```
    pub fn current_dir() -> Result<Self, CurrentDirError> {
        AbsUtf8PathBuf::current_dir().map(Self::new)
    }

    /// Creates a new `PathAnchor` at the process's logical current directory
    /// if it can be determined.
    ///
    /// Returns an error if the current directory could not be read, is not
    /// valid UTF-8, or is not absolute.
    ///
    /// # Notes
    ///
    /// This uses [`AbsUtf8PathBuf::logical_current_dir`]. If the `PWD`
    /// environment variable is set, it is checked first. It is verified to be
    /// absolute on the current platform, valid UTF-8, and to canonicalize to
    /// the same directory as the physical current directory. If any of these
    /// conditions are not met, the physical current directory is used instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::PathAnchor;
    ///
    /// let base = PathAnchor::logical_current_dir().unwrap();
    ///
    /// // The base names the same directory as the physical current
    /// // directory, though its spelling may differ if the directory was
    /// // entered through a symlink.
    /// assert_eq!(
    ///     std::fs::canonicalize(base.directory()).unwrap(),
    ///     std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
    /// );
    ///
    /// // Relative inputs are resolved against it and displayed as written.
    /// let path = base.resolve_input("config.toml").unwrap();
    /// assert_eq!(path.display().to_string(), "config.toml");
    /// ```
    pub fn logical_current_dir() -> Result<Self, CurrentDirError> {
        AbsUtf8PathBuf::logical_current_dir().map(Self::new)
    }

    /// Returns the base directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor, RelUtf8PathBuf};
    ///
    /// let base = PathAnchor::current_dir().unwrap();
    /// let config_name = RelUtf8PathBuf::new("config.toml").unwrap();
    ///
    /// // Use the directory to establish an absolute location without display spelling.
    /// let absolute = base.directory().join(&config_name);
    /// assert_eq!(
    ///     absolute.as_path(),
    ///     base.directory().as_path().join("config.toml"),
    /// );
    /// ```
    #[must_use]
    pub fn directory(&self) -> &AbsUtf8PathBuf {
        &self.directory
    }

    /// Resolves a relative path, preserving it for display.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor, RelUtf8PathBuf};
    ///
    /// let base = PathAnchor::current_dir().unwrap();
    /// let relative = RelUtf8PathBuf::new("../config.toml").unwrap();
    /// let expected = base.directory().join(&relative);
    /// let path = base.resolve_relative(relative);
    ///
    /// assert_eq!(path.absolute(), &expected);
    /// // Preserve the parent component supplied by the user for display.
    /// assert_eq!(path.display().to_string(), "../config.toml");
    /// ```
    #[must_use]
    pub fn resolve_relative(&self, relative: RelUtf8PathBuf) -> AnchoredPath {
        AnchoredPath {
            absolute: self.directory.join(&relative),
            relative: Some(relative),
        }
    }

    /// Uses an existing absolute location, but allows it to be displayed
    /// relative to this base.
    ///
    /// The provided path is stored unchanged. The relative form used for display
    /// is computed with [`AbsUtf8PathBuf::strip_prefix`]: if it returns `Some`,
    /// that relative path is displayed; otherwise, the absolute path is
    /// displayed.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor};
    ///
    /// let directory = AbsUtf8PathBuf::resolve_against_current_dir("project").unwrap();
    /// let base = PathAnchor::new(directory);
    ///
    /// // A path under the base is displayed relative to it.
    /// let absolute = AbsUtf8PathBuf::resolve_against_current_dir("project/config.toml").unwrap();
    /// let path = base.resolve_absolute(absolute.clone());
    /// assert_eq!(path.absolute(), &absolute);
    /// assert_eq!(path.display().to_string(), "config.toml");
    ///
    /// // An unrelated path is displayed as absolute.
    /// let outside = AbsUtf8PathBuf::resolve_against_current_dir("other.toml").unwrap();
    /// let path = base.resolve_absolute(outside.clone());
    /// assert_eq!(path.display().to_string(), outside.as_path().as_str());
    /// ```
    #[must_use]
    pub fn resolve_absolute(&self, absolute: AbsUtf8PathBuf) -> AnchoredPath {
        let relative = absolute.strip_prefix(&self.directory);
        AnchoredPath { absolute, relative }
    }

    /// Resolves an input path, using process state when necessary.
    ///
    /// This method uses the following logic:
    ///
    /// * If the path is a well-formed relative path, then use [`Self::resolve_relative`].
    /// * If the path is an absolute path, then use [`Self::resolve_absolute`].
    /// * On Windows and Cygwin, for drive-relative (`C:foo`) or root-relative
    ///   (`\foo` or `/foo`) paths, use [`AbsUtf8PathBuf::resolve_against_current_dir`],
    ///   which reads process state, then use [`Self::resolve_absolute`].
    /// * Otherwise (for empty inputs or inputs containing a NUL byte), the path is rejected.
    ///
    /// Returns an error for empty inputs, inputs containing a NUL byte, or if
    /// native resolution fails, including when the current directory is
    /// unavailable or the result is not valid UTF-8.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor, ResolvePathErrorKind};
    ///
    /// let base = PathAnchor::current_dir().unwrap();
    ///
    /// // Ordinary relative inputs use this base.
    /// let path = base.resolve_input("./config.toml").unwrap();
    /// assert!(path.absolute().as_path().is_absolute());
    /// assert_eq!(path.display().to_string(), "./config.toml");
    ///
    /// // Absolute inputs retain their location.
    /// let absolute = AbsUtf8PathBuf::resolve_against_current_dir("config.toml").unwrap();
    /// let path = base.resolve_input(&absolute).unwrap();
    /// assert_eq!(path.absolute(), &absolute);
    ///
    /// // Windows drive-relative inputs use native process state.
    /// #[cfg(windows)]
    /// {
    ///     let path = base.resolve_input(r"C:config.toml").unwrap();
    ///     assert!(path.absolute().as_path().is_absolute());
    ///     assert!(path.absolute().as_path().ends_with("config.toml"));
    /// }
    ///
    /// // Empty inputs are rejected.
    /// let error = base.resolve_input("").unwrap_err();
    /// match error.kind() {
    ///     ResolvePathErrorKind::Empty => {}
    ///     other => panic!("expected Empty, got {other:?}"),
    /// }
    /// ```
    pub fn resolve_input<P: AsRef<Utf8Path>>(
        &self,
        path: P,
    ) -> Result<AnchoredPath, ResolvePathError> {
        match classify_path(path.as_ref().to_owned()) {
            PathClass::Relative(relative) => Ok(self.resolve_relative(relative)),
            PathClass::Absolute(absolute) => Ok(self.resolve_absolute(absolute)),
            PathClass::RootRelative(path) | PathClass::DriveRelative(path) => {
                let absolute = AbsUtf8PathBuf::resolve_against_current_dir(&path)?;
                Ok(self.resolve_absolute(absolute))
            }
            PathClass::Malformed(path, kind) => Err(ResolvePathError::new(path, kind.into())),
        }
    }
}

/// An anchored path: an absolute path, along with a way to display it as a
/// relative path if possible.
///
/// Returned by the `resolve_*` methods on [`PathAnchor`].
///
/// # Notes
///
/// This type does not have a `Display` or `AsRef<Path>` implementation —
/// callers must explicitly choose between [`Self::absolute`] and
/// [`Self::display`].
///
/// For similar reasons, this type does not have `PartialEq`, `Eq`, `Hash`, or
/// `Ord` implementations.
///
/// # Examples
///
/// Use the absolute path for filesystem access, and the displayer for messages:
///
/// ```
/// use camino_anchored::{AbsUtf8PathBuf, PathAnchor, RelUtf8PathBuf};
///
/// let directory = camino_tempfile::tempdir()?;
/// std::fs::write(directory.path().join("config.toml"), "enabled = true")?;
/// let base = PathAnchor::new(AbsUtf8PathBuf::new(directory.path())?);
/// let path = base.resolve_relative(RelUtf8PathBuf::new("config.toml")?);
///
/// let contents = std::fs::read_to_string(path.absolute())?;
/// assert_eq!(contents, "enabled = true");
/// assert_eq!(format!("read {}", path.display()), "read config.toml");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// An `AnchoredPath` cannot be passed directly into a filesystem API. Use
/// [`Self::absolute`] to get the absolute path first.
///
/// ```compile_fail,E0277
/// use camino_anchored::AnchoredPath;
/// fn read(path: &AnchoredPath) {
///     std::fs::read(path).unwrap();
/// }
/// ```
#[derive(Clone, Debug)]
pub struct AnchoredPath {
    absolute: AbsUtf8PathBuf,
    relative: Option<RelUtf8PathBuf>,
}

impl AnchoredPath {
    /// Returns the absolute location of this path for filesystem access.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor};
    /// use std::fs;
    ///
    /// let directory = camino_tempfile::tempdir().unwrap();
    /// let base = PathAnchor::new(AbsUtf8PathBuf::new(directory.path()).unwrap());
    /// let path = base.resolve_input("config.toml").unwrap();
    ///
    /// // Pass the absolute path to filesystem APIs.
    /// fs::write(path.absolute(), "enabled = true").unwrap();
    /// let contents = fs::read_to_string(path.absolute()).unwrap();
    /// assert_eq!(contents, "enabled = true");
    /// ```
    #[must_use]
    pub fn absolute(&self) -> &AbsUtf8PathBuf {
        &self.absolute
    }

    /// Consumes self, returning the absolute path.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor};
    ///
    /// let base = PathAnchor::current_dir().unwrap();
    /// let path = base.resolve_input("config.toml").unwrap();
    ///
    /// // Keep only the absolute path once the display form is no longer needed.
    /// let absolute: AbsUtf8PathBuf = path.into_absolute();
    /// assert_eq!(
    ///     absolute.as_path(),
    ///     base.directory().as_path().join("config.toml"),
    /// );
    /// ```
    #[must_use]
    pub fn into_absolute(self) -> AbsUtf8PathBuf {
        self.absolute
    }

    /// Returns the relative form of this path, if one was retained.
    ///
    /// This is the form [`Self::display`] shows when it is available.
    ///
    /// The relative form is `Some` if this path was produced by
    /// [`PathAnchor::resolve_relative`], or by [`PathAnchor::resolve_absolute`]
    /// when [`AbsUtf8PathBuf::strip_prefix`] returns `Some`.
    ///
    /// # Notes
    ///
    /// The path is relative to the anchor it was resolved against, and may
    /// contain `..` components that escape the anchor. This type does not record the
    /// anchor itself.
    ///
    /// # Examples
    ///
    /// Map a source file to the corresponding location in an output tree:
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor};
    ///
    /// let source = PathAnchor::new(AbsUtf8PathBuf::resolve_against_current_dir("src").unwrap());
    /// let output = PathAnchor::new(AbsUtf8PathBuf::resolve_against_current_dir("target").unwrap());
    ///
    /// let input = source.resolve_input("widget/lib.rs").unwrap();
    /// let relative = input
    ///     .relative()
    ///     .expect("relative input retains its spelling");
    ///
    /// // Resolve the same relative location against the output anchor.
    /// let generated = output.resolve_relative(relative.clone());
    /// assert_eq!(
    ///     generated.absolute().as_path(),
    ///     output.directory().as_path().join("widget/lib.rs"),
    /// );
    /// assert_eq!(generated.display().to_string(), "widget/lib.rs");
    ///
    /// // An unrelated path does not have a relative form.
    /// let outside = source.resolve_absolute(output.directory().clone());
    /// assert_eq!(outside.relative(), None);
    /// assert_eq!(
    ///     outside.display().to_string(),
    ///     output.directory().to_string()
    /// );
    /// ```
    #[must_use]
    pub fn relative(&self) -> Option<&RelUtf8PathBuf> {
        self.relative.as_ref()
    }

    /// Displays the path relative to its original base when possible.
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{AbsUtf8PathBuf, PathAnchor};
    ///
    /// let base = PathAnchor::current_dir().unwrap();
    /// let path = base.resolve_input("./config.toml").unwrap();
    ///
    /// let message = format!("could not read {}", path.display());
    /// assert_eq!(message, "could not read ./config.toml");
    /// ```
    #[must_use]
    pub fn display(&self) -> DisplayPath<'_> {
        DisplayPath(
            self.relative
                .as_ref()
                .map_or(self.absolute.as_path(), RelUtf8PathBuf::as_path),
        )
    }
}

/// A formatting-only path adapter.
///
/// Returned by [`AnchoredPath::display`].
///
/// # Examples
///
/// ```
/// use camino_anchored::{AbsUtf8PathBuf, PathAnchor, RelUtf8PathBuf};
///
/// let base = PathAnchor::current_dir()?;
/// let path = base.resolve_relative(RelUtf8PathBuf::new("./config.toml")?);
/// let display = path.display();
///
/// assert_eq!(
///     format!("could not read {display}"),
///     "could not read ./config.toml"
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// The display adapter cannot be used for filesystem access:
///
/// ```compile_fail,E0277
/// use camino_anchored::AnchoredPath;
/// fn read(path: &AnchoredPath) {
///     std::fs::read(path.display()).unwrap();
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct DisplayPath<'a>(&'a Utf8Path);

impl<'a> DisplayPath<'a> {
    /// Returns the text that [`Display`](fmt::Display) would print.
    ///
    /// This is useful for structured logging, assertions, and other text
    /// processing.
    ///
    /// # Notes
    ///
    /// This type deliberately does not provide a [`Utf8Path`] view. The text
    /// is whichever of the relative or absolute forms was chosen for display,
    /// so treating it as a path is ambiguous, and passing it to a filesystem
    /// API would resolve a relative form against the process's current
    /// directory rather than its anchor.
    ///
    /// * For filesystem access, use [`AnchoredPath::absolute`].
    /// * To obtain the relative form, use [`AnchoredPath::relative`].
    ///
    /// # Examples
    ///
    /// ```
    /// use camino_anchored::{PathAnchor, RelUtf8PathBuf};
    ///
    /// let base = PathAnchor::current_dir().unwrap();
    /// let path = base.resolve_relative(RelUtf8PathBuf::new("./config.toml").unwrap());
    /// let display = path.display();
    ///
    /// assert_eq!(display.as_str(), "./config.toml");
    /// assert_eq!(display.as_str(), display.to_string());
    /// ```
    #[must_use]
    pub fn as_str(&self) -> &'a str {
        self.0.as_str()
    }
}

impl fmt::Display for DisplayPath<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ResolvePathErrorKind,
        test_helpers::{absolute, assert_resolve_error, relative},
    };
    use camino::Utf8PathBuf;

    #[cfg(unix)]
    const BASE: &str = "/repo";
    #[cfg(windows)]
    const BASE: &str = r"C:\repo";

    #[track_caller]
    fn base(directory: impl Into<Utf8PathBuf>) -> PathAnchor {
        PathAnchor::new(absolute(directory))
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_and_device_bases_preserve_relative_inputs() {
        for root in [r"\\?\C:\repo", r"\\?\UNC\server\share\repo", r"\\.\C:\repo"] {
            let input = relative("../file/");
            let resolved = base(root).resolve_relative(input.clone());
            assert_eq!(
                resolved.display().to_string(),
                input.to_string(),
                "{root:?}"
            );
            assert_eq!(resolved.relative(), Some(&input), "{root:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_anchor_displays_absolute_inputs_relative() {
        let anchor = base(r"\\?\C:\repo");
        for (path, expected) in [
            (r"\\?\C:\repo\src\lib.rs", r"src\lib.rs"),
            (r"\\?\C:\repo\a/b", r"\\?\C:\repo\a/b"),
        ] {
            let resolved = anchor.resolve_absolute(absolute(path));
            assert_eq!(resolved.display().to_string(), expected, "{path:?}");
        }
    }

    #[test]
    fn resolve_input_rejects_invalid_input() {
        let base = base(BASE);

        // NUL is checked before the other rejection kinds, so absolute inputs and
        // Windows drive- or root-relative inputs containing NUL must also be
        // rejected, not resolved against the current directory.
        let mut cases = vec![
            (String::new(), ResolvePathErrorKind::Empty),
            ("a\0b".to_owned(), ResolvePathErrorKind::ContainsNul),
            (format!("{BASE}/a\0b"), ResolvePathErrorKind::ContainsNul),
        ];
        if cfg!(windows) {
            cases.push(("C:a\0b".to_owned(), ResolvePathErrorKind::ContainsNul));
            cases.push(("\\a\0b".to_owned(), ResolvePathErrorKind::ContainsNul));
        }
        for (input, expected) in cases {
            assert_resolve_error(base.resolve_input(&input), &input, expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn parent_component_after_symlink_is_not_collapsed() {
        use std::{fs, os::unix::fs::symlink};

        let temp = camino_tempfile::tempdir().expect("created temp dir");
        let workspace = temp.path().join("workspace");
        let target = temp.path().join("target");
        fs::create_dir(&workspace).expect("created workspace");
        fs::create_dir_all(target.join("child")).expect("created target/child");

        // Create a symlink from workspace/link to target/child, so that
        // `link/..` represents `target`, not workspace.
        symlink(target.join("child"), workspace.join("link")).expect("created workspace/link");

        fs::write(workspace.join("config.toml"), "in workspace").expect("wrote workspace config");
        fs::write(target.join("config.toml"), "in target").expect("wrote target config");

        let resolved = base(&workspace).resolve_relative(relative("link/../config.toml"));

        assert_eq!(
            fs::read_to_string(resolved.absolute()).expect("read resolved config"),
            "in target",
        );
    }
}
