---
name: rust-checks
description: Use when verifying, testing, linting or "checking" this Rust repo before claiming work is done. Runs cargo fmt --check, cargo clippy --all-targets (must be zero warnings), cargo test, and the container-backed integration tests. Triggers on - test, tests, check, checks, lint, clippy, fmt, format, verify, CI, green, passing.
---

# Verifying this repo

Three gates, in increasing cost. Run all three before reporting a task complete.

```bash
cargo fmt --check                    # must produce no diff
cargo clippy --all-targets           # must produce zero warnings
cargo test                           # 94 unit/render + 19 skipped integration
```

If `just` is installed, `just check` runs all three and `just test-live` adds the
integration tests against a real Redis.

## Rules

- **Zero clippy warnings is the bar, not "no errors".** Do not add
  `#[allow(...)]` to silence a lint. Fix the code or, if the lint is genuinely
  wrong, allow it at the crate level in `Cargo.toml` with a reason.
- **Report failures verbatim.** Paste the actual compiler/test output. Never
  paraphrase a failure into "there was an issue with types" — the exact text is
  the only thing that makes a failure actionable.
- **A green `cargo build` is not a green build.** `cargo build` does not compile
  `#[cfg(test)]` code or the `tests/` directory, so it stays green while tests
  are broken. Always `cargo test`.
- **Run `cargo fmt` before `cargo fmt --check`**, otherwise `--check` just
  reports your own unformatted work.

## Integration tests

`tests/live_store.rs` is opt-in so `cargo test` can never touch a real
database. It skips itself unless `REDISUI_TEST_REDIS` is set.

```bash
# container on :6390, scratch database 15
REDISUI_TEST_REDIS=redis://127.0.0.1:6390/15 cargo test --test live_store
```

Use a throwaway database index. Tests isolate by key prefix, never `FLUSHDB`.

Run these whenever the change touches `src/store/**` — the RESP parsing and the
exact Redis commands sent are not covered by the unit tests in the same way, and
the fake connection cannot catch a wire-format mistake that a real server
rejects.

## When a test is wrong rather than the code

Fix the expectation, not the assertion, and make the expectation self-maintaining
where possible. `src/ui/text.rs` once hard-coded a fixture width that a
repo-wide rename silently changed; it now derives the value from the data:

```rust
assert_eq!(widths[1], width("short"));   // not: assert_eq!(widths[1], 5)
```

A test that fails right after an unrelated rename is usually telling you the
test is coupled to something incidental.
