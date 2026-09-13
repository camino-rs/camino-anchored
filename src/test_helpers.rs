// Copyright (c) The camino-anchored Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{AbsUtf8PathBuf, RelUtf8PathBuf, ResolvePathError, ResolvePathErrorKind};
use camino::Utf8PathBuf;
use std::{fmt, mem};

#[track_caller]
pub(crate) fn absolute(path: impl Into<Utf8PathBuf>) -> AbsUtf8PathBuf {
    AbsUtf8PathBuf::new(path).expect("path is absolute")
}

#[track_caller]
pub(crate) fn relative(path: &str) -> RelUtf8PathBuf {
    RelUtf8PathBuf::new(path).expect("path is a well-formed relative path")
}

#[track_caller]
pub(crate) fn assert_resolve_error<T: fmt::Debug>(
    result: Result<T, ResolvePathError>,
    input: &str,
    expected: ResolvePathErrorKind,
) {
    let error = result.expect_err("input is rejected");
    assert_eq!(error.input().as_str(), input);
    assert_eq!(
        mem::discriminant(error.kind()),
        mem::discriminant(&expected),
        "{input:?}: got {:?}",
        error.kind()
    );
}
