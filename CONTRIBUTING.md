# Contributing to ankro

Thanks for taking the time to work on `ankro`.
This document explains how to set up the repository, how to make changes safely, and what to include when you submit a patch.

## Scope

`ankro` is a small project with a clear runtime contract:

- a TCP bridge accepts requests
- requests are either executed immediately or queued
- a target executable responds to `-b` and `-r`
- queue state is persisted on disk

When you change behavior, keep the docs and the example app aligned with the code.

## Repository Layout

- `src/` contains the Rust implementation
- `example-app/` contains the browser demo and the test target
- `CHANGELOG.md` tracks notable releases
- `README.md` explains the system and operational behavior

## Getting Started

Recommended setup:

```bash
cargo build
node --version
```

The repository uses Rust and Node.js:

- Rust for the `ankro` binary
- Node.js for the example bridge and example target

If you are missing either toolchain, install it before making changes.

## Common Commands

Use these commands while developing:

```bash
cargo test
```

```bash
cargo fmt
```

```bash
cargo clippy
```

```bash
node --test example-app/protocol.test.mjs example-app/target.test.mjs
```

If you change runtime behavior in the example app, rerun the Node tests as well as the Rust tests.

## Coding Standards

- Keep code explicit and easy to trace.
- Prefer small, local changes over broad refactors unless the refactor is the point of the change.
- Preserve queue ordering and error semantics unless the change explicitly targets them.
- Add or update tests for behavioral changes.
- Use ASCII in code and docs unless a file already uses non-ASCII text.
- Keep comments short and informative. Explain intent, not syntax.

## Documentation Standards

When you add or change public behavior, update the documentation in the same patch.

Expected documentation updates:

- `README.md` for user-facing behavior
- `CONTRIBUTING.md` for development workflow changes
- rustdoc comments on public Rust APIs
- `example-app/README.md` when the demo flow changes

Good documentation is specific about:

- the protocol shape
- the queueing rules
- target resolution rules
- timeout and backpressure behavior
- failure modes and their meaning

## Rust Documentation

Public Rust items should have rustdoc comments that answer three questions:

1. What is this?
2. When should I use it?
3. What are the important constraints?

Focus on these items first:

- `serve`
- `busy`
- `Args`
- `Commands`
- `BanList`
- `RequestQueue`
- `DiskQueue`
- `PendingRequest`
- `StoredRequest`

## Testing Expectations

For most changes:

1. Run `cargo test`.
2. Run the Node tests if the example app or bridge behavior changed.
3. If you changed runtime behavior, test the manual happy path.

Examples of behavior that should have tests:

- target resolution
- queue ordering
- queue persistence
- banning behavior
- request serialization
- example target protocol handling

## Manual Verification

When relevant, verify the full chain:

```bash
./example-app/target
```

```bash
cargo run -- serve --port 1234 --target ./example-app/target --ban-threshold 1000
```

```bash
node example-app/server.mjs
```

Then exercise the browser app or the bridge endpoints and confirm:

- requests reach the target
- busy requests queue instead of failing
- queued requests eventually drain
- bans behave as expected

## Pull Request Checklist

Before opening a PR, make sure:

- the code builds
- tests pass
- docs reflect the new behavior
- public APIs have rustdoc comments
- the example app still works
- any new failure mode has a clear message

## Bug Reports

Include the following when reporting a bug:

- the command you ran
- the exact error message
- the target path you passed
- whether you used the example app or your own target
- the platform and Rust/Node versions if they are relevant

## Small Changes

For small bug fixes, a concise patch is fine.
Still include:

- tests if the behavior is observable
- docs if the user-facing behavior changed

## Larger Changes

For larger behavior changes, prefer breaking the work into:

1. code changes
2. tests
3. documentation
4. manual verification

That keeps review focused and makes regressions easier to spot.

