<!-- cargo-sync-rdme title [[ -->
# camino-anchored
<!-- cargo-sync-rdme ]] -->
<!-- cargo-sync-rdme badge [[ -->
![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/camino-anchored.svg?)
[![crates.io](https://img.shields.io/crates/v/camino-anchored.svg?logo=rust)](https://crates.io/crates/camino-anchored)
[![docs.rs](https://img.shields.io/docsrs/camino-anchored.svg?logo=docs.rs)](https://docs.rs/camino-anchored)
[![Rust: ^1.86.0](https://img.shields.io/badge/rust-^1.86.0-93450a.svg?logo=rust)](https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field)
<!-- cargo-sync-rdme ]] -->
<!-- cargo-sync-rdme rustdoc [[ -->
UTF-8 paths with explicit resolution and base-relative display.

## Motivation

Command-line tools often want to accept and display paths in a principled
manner. In general:

* Relative paths provided over the command line should be resolved against
  the current working directory.
* Internally, one might wish to always use absolute paths.
* When displaying paths, one might wish to preserve the spelling of
  relative paths the user provided, display paths within a chosen base
  directory relative to it, and fall back to absolute paths otherwise.

This crate provides a set of helper types to aid in handling paths
correctly.

### Relative and absolute paths

To represent paths, this crate provides two newtype wrappers around
[`Utf8PathBuf`]:

* [`AbsUtf8PathBuf`] represents an absolute path such as
  `/home/user` or `C:\Users\user`.
* [`RelUtf8PathBuf`] represents a relative path such as
  `foo/bar` or `../foo`.

(On Windows, there are paths that are neither entirely absolute nor entirely
relative, such as `C:foo` and `\Users\user`. These are accepted by neither
[`AbsUtf8PathBuf`] nor [`RelUtf8PathBuf`], but can be processed via
[`PathAnchor::resolve_input`].)

### Path resolution

For resolving and displaying paths, this crate introduces
[`PathAnchor`]. Some examples of path anchors a project
might use are: the cwd, a workspace root, or a repository root.

A [`PathAnchor`] can be used to turn a path into an [`AnchoredPath`].
An [`AnchoredPath`] carries the path in both absolute and relative
forms, if available, and has a [display helper][AnchoredPath::display] to
format the path for display.

## Examples

Let’s say you have the following directory structure:

````text
project/                         <- workspace root
├── .config/
│   └── myproject.toml
└── crates/
    ├── custom.toml
    └── widget/                  <- invocation directory
````

Let’s say the user cds to `project/crates/widget` (here, called the
*invocation directory*) and invokes your tool with `--config-file ../custom.toml`.

* The `../custom.toml` on the command line should be resolved relative
  to the invocation directory.
* The default config file should be resolved relative to the workspace root.
* The explicit input should retain its `../custom.toml` spelling for display.
* The discovered config should be displayed as an absolute path. One might
  imagine synthesizing the right number of `..` when possible, but the
  presence of symlinks makes that ambiguous.

````rust
use camino_anchored::{AbsUtf8PathBuf, PathAnchor, RelUtf8PathBuf};
use std::fs;

// Set up the directory layout mentioned above.
let temp_dir = camino_tempfile::tempdir()?;
let workspace_dir = temp_dir.path().join("project");
let invocation_dir = workspace_dir.join("crates/widget");
fs::create_dir_all(workspace_dir.join(".config"))?;
fs::create_dir_all(&invocation_dir)?;
fs::write(
    workspace_dir.join(".config/myproject.toml"),
    "source = 'repository'",
)?;
fs::write(
    workspace_dir.join("crates/custom.toml"),
    "source = 'explicit'",
)?;

// Create PathAnchor instances for the workspace and invocation
// directories.
let workspace_base = PathAnchor::new(AbsUtf8PathBuf::new(&workspace_dir)?);
let invocation_base = PathAnchor::new(AbsUtf8PathBuf::new(&invocation_dir)?);

// Turn the command line input into an AnchoredPath by resolving it against the
// invocation directory.
let explicit_config = invocation_base.resolve_input("../custom.toml")?;
assert_eq!(
    explicit_config.absolute().as_path(),
    invocation_dir.join("../custom.toml"),
);
// `explicit_config.display()` preserves the user's `..`, since that was
// provided as input.
assert_eq!(explicit_config.display().to_string(), "../custom.toml");

// Locate the default config, which is relative to the workspace root.
let default_config_relative = RelUtf8PathBuf::new(".config/myproject.toml")?;
let default_config_absolute = workspace_base.directory().join(&default_config_relative);

// Turn the default config path into an AnchoredPath against the invocation
// directory.
let default_config = invocation_base.resolve_absolute(default_config_absolute);
assert_eq!(
    default_config.absolute().as_path(),
    workspace_dir.join(".config/myproject.toml"),
);

// The default config's path doesn't start with the invocation directory's
// path, so we display it as an absolute path. (See
// `AbsUtf8PathBuf::strip_prefix` for the exact rules.)
assert_eq!(
    default_config.display().to_string(),
    default_config.absolute().as_path().as_str(),
);

for config_path in [&explicit_config, &default_config] {
    // To access a file using its absolute path, use `AnchoredPath::absolute`.
    let contents = fs::read_to_string(config_path.absolute())?;
    // To display a path, use `AnchoredPath::display`.
    println!("read {}: {contents}", config_path.display());
}

// Paths mentioned inside a config file are typically relative to the file's
// own directory. Use `AbsUtf8PathBuf::parent` to derive that anchor.
let config_dir = explicit_config
    .absolute()
    .parent()
    .expect("config file has a parent directory");
let config_base = PathAnchor::new(config_dir);
let data_file = config_base.resolve_relative(RelUtf8PathBuf::new("data/input.txt")?);
assert_eq!(
    data_file.absolute().as_path(),
    invocation_dir.join("../data/input.txt"),
);
````

## Minimum supported Rust version (MSRV)

This crate’s MSRV is **Rust 1.86**. In general we aim for 6 months of Rust
compatibility.

[`Utf8PathBuf`]: https://docs.rs/camino/1.2.5/camino/struct.Utf8PathBuf.html "struct camino::Utf8PathBuf"
[`AbsUtf8PathBuf`]: https://docs.rs/camino-anchored/0.1.0/camino_anchored/paths/struct.AbsUtf8PathBuf.html "struct camino_anchored::paths::AbsUtf8PathBuf"
[`RelUtf8PathBuf`]: https://docs.rs/camino-anchored/0.1.0/camino_anchored/paths/struct.RelUtf8PathBuf.html "struct camino_anchored::paths::RelUtf8PathBuf"
[`PathAnchor::resolve_input`]: https://docs.rs/camino-anchored/0.1.0/camino_anchored/resolution/struct.PathAnchor.html#method.resolve_input "method camino_anchored::resolution::PathAnchor::resolve_input"
[`PathAnchor`]: https://docs.rs/camino-anchored/0.1.0/camino_anchored/resolution/struct.PathAnchor.html "struct camino_anchored::resolution::PathAnchor"
[`AnchoredPath`]: https://docs.rs/camino-anchored/0.1.0/camino_anchored/resolution/struct.AnchoredPath.html "struct camino_anchored::resolution::AnchoredPath"
[AnchoredPath::display]: https://docs.rs/camino-anchored/0.1.0/camino_anchored/resolution/struct.AnchoredPath.html#method.display "method camino_anchored::resolution::AnchoredPath::display"
<!-- cargo-sync-rdme ]] -->

## Development

Regenerate this README from the crate documentation with `just generate-readmes`.
This requires `just`, `cargo-sync-rdme`, and the `nightly-2026-09-13` Rust toolchain.

Run `cargo nextest run`, `cargo test --doc`, `cargo clippy --all-targets`, `just rustdoc`,
and `cargo xfmt`.

Licensed under MIT OR Apache-2.0.
