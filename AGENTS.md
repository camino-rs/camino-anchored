# Agent instructions

## Constraints

- Tests are run with `cargo nextest run` (plus `cargo test --doc` for doctests). `cargo test` is not supported for the test suite: tests may rely on nextest's process-per-test model (for example, tests that change the current directory). Do not add workarounds for `cargo test`, such as mutexes around current-directory changes.
