// Copyright (c) The camino-anchored Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use camino_anchored::{AbsUtf8PathBuf, PathAnchor, RelUtf8PathBuf, RelUtf8PathErrorKind};
use hegel::generators::{self, Generator};

// Using a small set of tokens is likely to result in more collisions, which is
// desirable for these tests.
#[cfg(unix)]
const TOKENS: &[&str] = &["/", "a", "b", ".", ".."];
#[cfg(windows)]
const TOKENS: &[&str] = &["/", "\\", "a", "b", ".", "..", ":", "C"];

// TODO-RAINCLAUDE: ROOTS excludes `\\?\` roots because join normalizes onto them, breaking exact-spelling strip_prefix properties; VERBATIM_ROOTS adds them to properties that don't assume exact spelling.
#[cfg(unix)]
const ROOTS: &[&str] = &["/", "//", "///"];
#[cfg(windows)]
const ROOTS: &[&str] = &[r"C:\", "C:/", r"\\server\share\", r"\\.\C:\"];

#[cfg(unix)]
const VERBATIM_ROOTS: &[&str] = &[];
#[cfg(windows)]
const VERBATIM_ROOTS: &[&str] = &[r"\\?\C:\", r"\\?\UNC\server\share\"];

#[hegel::composite]
fn path_text(tc: &hegel::TestCase) -> String {
    let tokens: Vec<&str> = tc.draw(generators::vecs(generators::sampled_from(TOKENS.to_vec())));
    tokens.concat()
}

fn draw_absolute_path(tc: &hegel::TestCase, roots: &[&'static str]) -> AbsUtf8PathBuf {
    let root = tc.draw(generators::sampled_from(roots.to_vec()));
    let rest = tc.draw(path_text());
    AbsUtf8PathBuf::new(format!("{root}{rest}")).expect("a root followed by any text is absolute")
}

#[hegel::composite]
fn absolute_paths(tc: &hegel::TestCase) -> AbsUtf8PathBuf {
    draw_absolute_path(tc, ROOTS)
}

#[hegel::composite]
fn any_absolute_paths(tc: &hegel::TestCase) -> AbsUtf8PathBuf {
    draw_absolute_path(tc, &[ROOTS, VERBATIM_ROOTS].concat())
}

#[cfg(windows)]
const VERBATIM_NAMES: &[&str] = &["a", "b", "file.", "file ", "ab:c"];

#[cfg(windows)]
#[hegel::test(test_cases = 1000)]
fn strip_prefix_keeps_normal_suffixes_on_verbatim_paths(tc: hegel::TestCase) {
    let root = tc.draw(generators::sampled_from(VERBATIM_ROOTS.to_vec()));
    let base_names =
        tc.draw(generators::vecs(generators::sampled_from(VERBATIM_NAMES.to_vec())).max_size(2));
    let suffix_names = tc.draw(
        generators::vecs(generators::sampled_from(VERBATIM_NAMES.to_vec()))
            .min_size(1)
            .max_size(3),
    );
    let trailing = if tc.draw(generators::booleans()) {
        "\\"
    } else {
        ""
    };

    let base_text = format!("{root}{}", base_names.join("\\"));
    let suffix = format!("{}{trailing}", suffix_names.join("\\"));
    let path_text = if base_text.ends_with('\\') {
        format!("{base_text}{suffix}")
    } else {
        format!("{base_text}\\{suffix}")
    };
    let base =
        AbsUtf8PathBuf::new(base_text).expect("a verbatim root followed by names is absolute");
    let path =
        AbsUtf8PathBuf::new(path_text).expect("a verbatim root followed by names is absolute");

    assert_eq!(
        path.strip_prefix(&base)
            .as_ref()
            .map(|stripped| stripped.as_path().as_str()),
        Some(suffix.as_str()),
        "base: {base:?}, path: {path:?}"
    );
}

#[hegel::composite]
fn relative_paths(tc: &hegel::TestCase) -> RelUtf8PathBuf {
    let text = tc.draw(path_text());
    match RelUtf8PathBuf::new(text) {
        Ok(relative) => relative,
        Err(_) => tc.reject(),
    }
}

#[hegel::composite]
fn inputs(tc: &hegel::TestCase) -> String {
    tc.draw(hegel::one_of!(generators::text(), path_text()))
}

#[cfg(unix)]
fn has_double_root(path: &camino::Utf8Path) -> bool {
    path.as_str().starts_with("//") && !path.as_str().starts_with("///")
}

#[hegel::test]
fn constructors_preserve_spelling_and_are_mutually_exclusive(tc: hegel::TestCase) {
    let input = tc.draw(inputs());
    let absolute = AbsUtf8PathBuf::new(input.as_str());
    let relative = RelUtf8PathBuf::new(input.as_str());
    assert!(
        absolute.is_err() || relative.is_err(),
        "{input:?} was accepted as both absolute and relative"
    );
    #[cfg(unix)]
    if !input.is_empty() && !input.contains('\0') {
        let is_absolute = input.starts_with('/');
        assert_eq!(absolute.is_ok(), is_absolute, "{input:?}");
        assert_eq!(relative.is_ok(), !is_absolute, "{input:?}");
    }

    match absolute {
        Ok(path) => assert_eq!(path.as_path().as_str(), input),
        Err(error) => assert_eq!(error.path().as_str(), input),
    }
    match relative {
        Ok(path) => assert_eq!(path.as_path().as_str(), input),
        Err(error) => assert_eq!(error.path().as_str(), input),
    }
}

/// Predicts what `AbsUtf8PathBuf::strip_prefix` returns when stripping `base`
/// from `base.join(relative)`, without calling it.
///
/// The result is related to `relative`, except that leading `.` components (a
/// `.` followed by separators or by the end of the text) are removed, along
/// with the separators after them. This mirrors how `Utf8Path::strip_prefix`
/// skips such components at the front of its remainder. Everything from the
/// first other component onwards is kept as written, including trailing
/// separators and `.` components. If nothing is left, the result is `.`.
///
/// On Windows, removing a leading `.\` can expose a drive-relative path (for
/// example, `.\C:a` becomes `C:a`), which `strip_prefix` rejects. The oracle
/// returns `None` in that case.
///
/// This oracle is only correct for bases drawn from `ROOTS`. Joining onto a
/// verbatim (`\\?\`) base rewrites the path, so the joined text no longer
/// contains `relative`.
fn strip_prefix_oracle(relative: &RelUtf8PathBuf) -> Option<&str> {
    let mut rest = relative.as_path().as_str();
    while let Some(after) = rest
        .strip_prefix('.')
        .filter(|after| after.is_empty() || after.starts_with(std::path::is_separator))
    {
        rest = after.trim_start_matches(std::path::is_separator);
    }
    if rest.is_empty() {
        Some(".")
    } else {
        RelUtf8PathBuf::new(rest).ok().map(|_| rest)
    }
}

#[hegel::test(test_cases = 1000)]
fn strip_prefix_inverts_join(tc: hegel::TestCase) {
    let base = tc.draw(absolute_paths().print_as_debug());
    let relative = tc.draw(relative_paths().print_as_debug());

    let joined = base.join(&relative);
    let stripped = joined.strip_prefix(&base);
    assert_eq!(
        stripped.as_ref().map(|path| path.as_path().as_str()),
        strip_prefix_oracle(&relative),
        "joined: {joined:?}"
    );
}

#[cfg(unix)]
const SEPARATORS: &[&str] = &["/"];
#[cfg(windows)]
const SEPARATORS: &[&str] = &["/", "\\"];

#[cfg(unix)]
const BASE_RESPELLINGS: &[&str] = &["/", "/."];
#[cfg(windows)]
const BASE_RESPELLINGS: &[&str] = &["/", "/.", "\\", "\\."];

#[hegel::test(test_cases = 1000)]
fn strip_prefix_ignores_base_spelling(tc: hegel::TestCase) {
    let base = tc.draw(absolute_paths().print_as_debug());
    // Appending a separator to a root-only base changes the root (`/` -> `//`),
    // so give such bases a normal component.
    let base = if base.as_path().parent().is_some() {
        base
    } else {
        base.join(&RelUtf8PathBuf::new("a").expect("`a` is a well-formed relative path"))
    };
    let extras = tc.draw(
        generators::vecs(generators::sampled_from(BASE_RESPELLINGS.to_vec()))
            .min_size(1)
            .max_size(4),
    );
    let relative = tc.draw(relative_paths().print_as_debug());

    let respelled = AbsUtf8PathBuf::new(format!("{}{}", base.as_path(), extras.concat()))
        .expect("extending an absolute path keeps it absolute");
    assert_eq!(respelled, base);

    let path = base.join(&relative);
    let expected = strip_prefix_oracle(&relative);
    for anchor in [&base, &respelled] {
        assert_eq!(
            path.strip_prefix(anchor)
                .as_ref()
                .map(|stripped| stripped.as_path().as_str()),
            expected,
            "anchor: {anchor:?}, path: {path:?}"
        );
    }
}

#[hegel::test(test_cases = 1000)]
fn strip_prefix_removes_all_separators_after_base(tc: hegel::TestCase) {
    let base = tc.draw(absolute_paths().print_as_debug());
    // Extra separators after a root-only base change the root (e.g. `/` -> `//`
    // on Unix), so give such bases a normal component.
    let base = if base.as_path().parent().is_some() {
        base
    } else {
        base.join(&RelUtf8PathBuf::new("a").expect("`a` is a well-formed relative path"))
    };
    let separators = tc.draw(
        generators::vecs(generators::sampled_from(SEPARATORS.to_vec()))
            .min_size(1)
            .max_size(3),
    );
    let relative = tc.draw(relative_paths().print_as_debug());

    let path = AbsUtf8PathBuf::new(format!(
        "{}{}{}",
        base.as_path(),
        separators.concat(),
        relative.as_path()
    ))
    .expect("extending an absolute path keeps it absolute");
    assert_eq!(
        path.strip_prefix(&base)
            .as_ref()
            .map(|stripped| stripped.as_path().as_str()),
        strip_prefix_oracle(&relative),
        "path: {path:?}"
    );
}

#[hegel::test(test_cases = 1000)]
fn relative_display_names_the_same_path(tc: hegel::TestCase) {
    let base = tc.draw(any_absolute_paths().print_as_debug());
    let path = if tc.draw(generators::booleans()) {
        let suffix = tc.draw(path_text());
        AbsUtf8PathBuf::new(format!("{}{suffix}", base.as_path()))
            .expect("extending an absolute path keeps it absolute")
    } else {
        tc.draw(any_absolute_paths().print_as_debug())
    };

    let resolved = PathAnchor::new(base.clone()).resolve_absolute(path.clone());
    let display = resolved.display().to_string();
    assert_eq!(resolved.display().as_str(), display);

    let stripped = path.strip_prefix(&base);
    assert_eq!(resolved.relative(), stripped.as_ref());
    let Some(relative) = stripped else {
        assert_eq!(display, path.as_path().as_str());
        return;
    };
    assert_eq!(display, relative.as_path().as_str());

    let rejoined = base.join(&relative);
    assert_eq!(
        rejoined.as_path(),
        path.as_path(),
        "{relative:?} joined onto {base:?}"
    );
    if relative.as_path().as_str() != "." {
        assert!(
            path.as_path()
                .as_str()
                .ends_with(relative.as_path().as_str()),
            "{path:?} ends with {relative:?}"
        );
    }
    #[cfg(unix)]
    assert_eq!(
        has_double_root(rejoined.as_path()),
        has_double_root(path.as_path()),
        "{relative:?} joined onto {base:?} keeps the POSIX root of {path:?}"
    );
}

#[hegel::test]
fn resolve_relative_preserves_input_spelling(tc: hegel::TestCase) {
    let base = tc.draw(any_absolute_paths().print_as_debug());
    let relative = tc.draw(relative_paths().print_as_debug());

    let resolved = PathAnchor::new(base.clone()).resolve_relative(relative.clone());
    assert_eq!(resolved.display().to_string(), relative.as_path().as_str());
    assert_eq!(resolved.display().as_str(), relative.as_path().as_str());
    assert_eq!(resolved.relative(), Some(&relative));

    #[cfg(unix)]
    {
        let separator = if base.as_path().as_str().ends_with('/') {
            ""
        } else {
            "/"
        };
        assert_eq!(
            resolved.absolute().as_path().as_str(),
            format!("{}{separator}{}", base.as_path(), relative.as_path()),
        );
    }
}

#[hegel::test]
fn resolve_input_matches_resolve_relative_or_resolve_absolute(tc: hegel::TestCase) {
    let base = PathAnchor::new(tc.draw(any_absolute_paths().print_as_debug()));
    let input = tc.draw(inputs());

    let expected = match RelUtf8PathBuf::new(input.as_str()) {
        Ok(relative) => base.resolve_relative(relative),
        Err(error) => match error.kind() {
            RelUtf8PathErrorKind::Absolute => base.resolve_absolute(
                AbsUtf8PathBuf::new(input.as_str())
                    .expect("input classified as absolute is absolute"),
            ),
            RelUtf8PathErrorKind::Empty | RelUtf8PathErrorKind::ContainsNul => {
                base.resolve_input(&input)
                    .expect_err("empty inputs and inputs containing NUL are rejected");
                return;
            }
            RelUtf8PathErrorKind::RootRelative | RelUtf8PathErrorKind::DriveRelative => tc.reject(),
            other => panic!("unexpected relative path error kind: {other:?}"),
        },
    };

    let resolved = base
        .resolve_input(&input)
        .expect("well-formed input resolves");
    assert_eq!(
        resolved.absolute().as_path().as_str(),
        expected.absolute().as_path().as_str(),
    );
    assert_eq!(
        resolved.display().to_string(),
        expected.display().to_string()
    );
}
