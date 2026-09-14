// Copyright (c) The camino-anchored Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! UTF-8 paths with explicit resolution and base-relative display.
//!
//! # Motivation
//!
//! Command-line tools often want to accept and display paths in a principled
//! manner. In general:
//!
//! * Relative paths provided over the command line should be resolved against
//!   the current working directory.
//! * Internally, one might wish to always use absolute paths.
//! * When displaying paths, one might wish to preserve the spelling of
//!   relative paths the user provided, display paths within a chosen base
//!   directory relative to it, and fall back to absolute paths otherwise.
//!
//! This crate provides a set of helper types to aid in handling paths
//! correctly.
//!
//! ## Relative and absolute paths
//!
//! To represent paths, this crate provides two newtype wrappers around
//! [`Utf8PathBuf`]:
//!
//! * [`AbsUtf8PathBuf`] represents an absolute path such as
//!   `/home/user` or `C:\Users\user`.
//! * [`RelUtf8PathBuf`] represents a relative path such as
//!   `foo/bar` or `../foo`.
//!
//! (On Windows, there are paths that are neither entirely absolute nor entirely
//! relative, such as `C:foo` and `\Users\user`. These are accepted by neither
//! [`AbsUtf8PathBuf`] nor [`RelUtf8PathBuf`], but can be processed via
//! [`PathAnchor::resolve_input`].)
//!
//! ## Path resolution
//!
//! For resolving and displaying paths, this crate introduces
//! [`PathAnchor`]. Some examples of path anchors a project
//! might use are: the cwd, a workspace root, or a repository root.
//!
//! A [`PathAnchor`] can be used to turn a path into an [`AnchoredPath`].
//! An [`AnchoredPath`] carries the path in both absolute and relative
//! forms, if available, and has a [display helper][AnchoredPath::display] to
//! format the path for display.
//!
//! # Examples
//!
//! Let's say you have the following directory structure:
//!
//! ```text
//! project/                         <- workspace root
//! ├── .config/
//! │   └── myproject.toml
//! └── crates/
//!     ├── custom.toml
//!     └── widget/                  <- invocation directory
//! ```
//!
//! Let's say the user cds to `project/crates/widget` (here, called the
//! _invocation directory_) and invokes your tool with `--config-file
//! ../custom.toml`.
//!
//! * The `../custom.toml` on the command line should be resolved relative
//!   to the invocation directory.
//! * The default config file should be resolved relative to the workspace root.
//! * The explicit input should retain its `../custom.toml` spelling for display.
//! * The discovered config should be displayed as an absolute path. One might
//!   imagine synthesizing the right number of `..` when possible, but the
//!   presence of symlinks makes that ambiguous.
//!
//! ```
//! use camino_anchored::{AbsUtf8PathBuf, PathAnchor, RelUtf8PathBuf};
//! use std::fs;
//!
//! // Set up the directory layout mentioned above.
//! let temp_dir = camino_tempfile::tempdir()?;
//! let workspace_dir = temp_dir.path().join("project");
//! let invocation_dir = workspace_dir.join("crates/widget");
//! fs::create_dir_all(workspace_dir.join(".config"))?;
//! fs::create_dir_all(&invocation_dir)?;
//! fs::write(
//!     workspace_dir.join(".config/myproject.toml"),
//!     "source = 'repository'",
//! )?;
//! fs::write(
//!     workspace_dir.join("crates/custom.toml"),
//!     "source = 'explicit'",
//! )?;
//!
//! // Create PathAnchor instances for the workspace and invocation
//! // directories.
//! let workspace_base = PathAnchor::new(AbsUtf8PathBuf::new(&workspace_dir)?);
//! let invocation_base = PathAnchor::new(AbsUtf8PathBuf::new(&invocation_dir)?);
//!
//! // Turn the command line input into an AnchoredPath by resolving it against the
//! // invocation directory.
//! let explicit_config = invocation_base.resolve_input("../custom.toml")?;
//! assert_eq!(
//!     explicit_config.absolute().as_path(),
//!     invocation_dir.join("../custom.toml"),
//! );
//! // `explicit_config.display()` preserves the user's `..`, since that was
//! // provided as input.
//! assert_eq!(explicit_config.display().to_string(), "../custom.toml");
//!
//! // Locate the default config, which is relative to the workspace root.
//! let default_config_relative = RelUtf8PathBuf::new(".config/myproject.toml")?;
//! let default_config_absolute = workspace_base.directory().join(&default_config_relative);
//!
//! // Turn the default config path into an AnchoredPath against the invocation
//! // directory.
//! let default_config = invocation_base.resolve_absolute(default_config_absolute);
//! assert_eq!(
//!     default_config.absolute().as_path(),
//!     workspace_dir.join(".config/myproject.toml"),
//! );
//!
//! // The default config's path doesn't start with the invocation directory's
//! // path, so we display it as an absolute path. (See
//! // `AbsUtf8PathBuf::strip_prefix` for the exact rules.)
//! assert_eq!(
//!     default_config.display().to_string(),
//!     default_config.absolute().as_path().as_str(),
//! );
//!
//! for config_path in [&explicit_config, &default_config] {
//!     // To access a file using its absolute path, use `AnchoredPath::absolute`.
//!     let contents = fs::read_to_string(config_path.absolute())?;
//!     // To display a path, use `AnchoredPath::display`.
//!     println!("read {}: {contents}", config_path.display());
//! }
//!
//! // Paths mentioned inside a config file are typically relative to the file's
//! // own directory. Use `AbsUtf8PathBuf::parent` to derive that anchor.
//! let config_dir = explicit_config
//!     .absolute()
//!     .parent()
//!     .expect("config file has a parent directory");
//! let config_base = PathAnchor::new(config_dir);
//! let data_file = config_base.resolve_relative(RelUtf8PathBuf::new("data/input.txt")?);
//! assert_eq!(
//!     data_file.absolute().as_path(),
//!     invocation_dir.join("../data/input.txt"),
//! );
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Minimum supported Rust version (MSRV)
//!
//! This crate's MSRV is **Rust 1.86**. In general we aim for 6 months of Rust
//! compatibility.
//!
//! [`Utf8PathBuf`]: camino::Utf8PathBuf

mod errors;
mod paths;
mod resolution;
#[cfg(test)]
mod test_helpers;

pub use errors::{
    AbsUtf8PathError, AbsUtf8PathErrorKind, CurrentDirError, NativePathErrorKind, RelUtf8PathError,
    RelUtf8PathErrorKind, ResolvePathError, ResolvePathErrorKind, TryFromPathBufError,
};
pub use paths::{AbsUtf8PathBuf, RelUtf8PathBuf};
pub use resolution::{AnchoredPath, DisplayPath, PathAnchor};
