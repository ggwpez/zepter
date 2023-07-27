# Zepter

[![Rust](https://github.com/ggwpez/zepter/actions/workflows/rust.yml/badge.svg)](https://github.com/ggwpez/zepter/actions/workflows/rust.yml)
[![crates.io](https://img.shields.io/crates/v/zepter.svg)](https://crates.io/crates/zepter)
![MSRV](https://img.shields.io/badge/MSRV-1.65-informational)
[![docs.rs](https://img.shields.io/docsrs/zepter)](https://docs.rs/zepter/latest/zepter)

Analyze and fix feature propagation in your Rust workspace. The goal of this tool is to automatically lint and fix feature propagation in CI runs.

## Install

```bash
cargo install -f zepter --locked
```

## Example - Fixing feature propagation

Let's check that the `runtime-benchmarks` feature is properly passed down to all the dependencies of the `frame-support` crate in the workspace of [Substrate]. You can use commit `395853ac15` to verify it yourself:  

```bash
zepter lint propagate-feature --feature runtime-benchmarks -p frame-support --workspace
```

The output reveals that some dependencies expose the feature but don't get it passed down:  

```pre
crate 'frame-support'
  feature 'runtime-benchmarks'
    must propagate to:
      frame-system
      sp-runtime
      sp-staking
Found 3 issues and fixed 0 issues.
```

Without the `-p` it will detect many more problems. You can verify this for the [frame-support](https://github.com/paritytech/substrate/blob/ce2cee35f8f0fc5968ea6ffaffa6660dcd008804/frame/support/Cargo.toml#L71) which is indeed missing the feature for `sp-runtime` while [sp-runtime](https://github.com/paritytech/substrate/blob/0b6aec52a90870c999856cd37f7d04789cdd8dfc/primitives/runtime/Cargo.toml#L43) clearly supports it 🤔.

This can be fixed by appending the `--fix` flag, which results in this diff:

```patch
-runtime-benchmarks = []
+runtime-benchmarks = [
+       "frame-system/runtime-benchmarks",
+       "sp-runtime/runtime-benchmarks",
+       "sp-staking/runtime-benchmarks",
+]
```

The auto-fix always enables the features of optional dependencies as optional features. This is a noninvasive default, but may not correctly enable the dependency itself.

## Example - Feature tracing

Let's say you want to ensure that specific features are never enabled by default. For this example, we will use the `try-runtime` feature of [Substrate]. Check out branch `oty-faulty-feature-demo` and try:

```bash
zepter lint never-implies --precondition default --stays-disabled try-runtime --offline --workspace
```

The `precondition` defines the feature on the left side of the implication and `stays-disabled` expressing that the precondition never enables this.

Errors correctly with:
```pre
Feature 'default' implies 'try-runtime' via path:
  frame-benchmarking/default -> frame-benchmarking/std -> frame-system/std -> frame-support/wrong -> frame-support/wrong2 -> frame-support/try-runtime
```

Only the first path is shown in case there are multiple.

## Example - Dependency tracing

Recently there was a build error in the [Substrate](https://github.com/paritytech/substrate) master CI which was caused by a downstream dependency [`snow`](https://github.com/mcginty/snow/issues/146). To investigate this, it is useful to see *how* Substrate depends on it.  

Let's find out how `node-cli` depends on `snow` (example on commit `dd6aedee3b8d5`):

```bash
zepter trace node-cli snow
```

It reports that `snow` is pulled in from libp2p - good to know. In this case, all paths are displayed.

```pre
node-cli -> try-runtime-cli -> substrate-rpc-client -> sc-rpc-api -> sc-chain-spec -> sc-telemetry -> libp2p -> libp2p-webrtc -> libp2p-noise -> snow
```

## Testing

UI and integration tests are run with the normal `cargo test`.  
Environment overwrites exist for:
- `OVERWRITE`: Update the `cout` and `diff` locks.
- `UI_FILTER`: Regex to selectively run files.
- `KEEP_GOING`: Print `FAILED` but don't abort. TODO: It's buggy

## Roadmap

- [x] Add feature information to the enabled deps
- [ ] Allow manual skipping of dev dependencies (currently always skipped)
- [ ] Introduce filters for versions and features for arguments `to``
- [x] Optimize `shortest_path` function
- [ ] Create lint rules which can be used to validate that certain constraints in the work-space hold

<!-- LINKS -->
[Cumulus]: https://github.com/paritytech/cumulus
[Substrate]: https://github.com/paritytech/substrate
