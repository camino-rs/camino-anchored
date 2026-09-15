# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->
## Unreleased - ReleaseDate

### Changed

- `PathAnchor::resolve_relative` now keeps the relative path (returned by `AnchoredPath::relative` and used for display) when the anchor is a Windows verbatim (`\\?\`) or device namespace (`\\.\`) path.
- `AbsUtf8PathBuf::strip_prefix`, and therefore `PathAnchor::resolve_absolute`, now matches `base` by components. Equal bases with different spellings, such as `/repo//`, `/repo/.`, `C:/repo`, or `c:\repo`, now produce a relative path instead of `None`. Leading `.` components after `base` are no longer included in the result.
- `AbsUtf8PathBuf::strip_prefix`, and therefore `PathAnchor::resolve_absolute`, now produces relative paths for Windows verbatim (`\\?\`) paths, such as those returned by `std::fs::canonicalize`, and device namespace (`\\.\`) paths. A verbatim path is still displayed as absolute if its remainder contains `/`, `.`, or `..`.

## [0.1.0] - 2026-09-14

Initial release.

<!-- next-url -->
[0.1.0]: https://github.com/camino-rs/camino-anchored/releases/tag/camino-anchored-0.1.0
