set positional-arguments

# Note: help messages should be 1 line long as required by just.

# Print a help message.
help:
    just --list

# Run `cargo hack --feature-powerset` on crates
powerset *args:
    cargo hack --feature-powerset --workspace "$@"

# Build rustdoc for the crate, treating warnings as errors
rustdoc *args:
    RUSTDOCFLAGS="${RUSTDOCFLAGS:-} -D warnings" cargo doc --no-deps --all-features "$@"

# Generate README.md files using `cargo-sync-rdme`.
generate-readmes:
    cargo sync-rdme --toolchain nightly-2026-09-13 --workspace --all-features

# Run cargo release in CI.
ci-cargo-release package:
    # cargo-release requires a release off a branch.
    git checkout -B to-release
    cargo release publish --publish --execute --no-confirm --package {{package}}
    git checkout -
    git branch -D to-release
