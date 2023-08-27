# Zepter

[![Rust](https://github.com/ggwpez/zepter/actions/workflows/rust.yml/badge.svg)](https://github.com/ggwpez/zepter/actions/workflows/rust.yml)
[![crates.io](https://img.shields.io/crates/v/zepter.svg)](https://crates.io/crates/zepter)
![MSRV](https://img.shields.io/badge/MSRV-1.70-informational)
[![docs.rs](https://img.shields.io/docsrs/zepter)](https://docs.rs/zepter/latest/zepter)

Analyze, Fix and Format features in your Rust workspace. The goal of this tool is to have this CI ready to prevent common errors with Rust features.

## Install

```bash
cargo install -f zepter --locked
```

## Commands

zepter
- format
  - features: Format features layout and remove duplicates.
- trace: Trace dependencies paths.
- lint
  - propagate-features: Check that features are passed down.
  - never-enables: A feature should never enable another other.
  - never-implies *(⚠️ unstable)*: A feature should never transitively imply another one.
  - only-enables *(⚠️ unstable)*: A features should exclusively enable another one.
  - why-enables *(⚠️ unstable)*: Find out why a specific feature is enables.

## Example - Feature Formatting

To ensure that your features are in canonical formatting, just run:

```bash
zepter format features --check
# Or shorter:
zepter f f -c
```

The output will tell you which features are missing formatting:

```pre
Found 3 crates with unformatted features:
  polkadot-cli
  polkadot-runtime-common
  polkadot-runtime-parachains
  ...
Run again without --check to format them.
```

You can then re-run without the `check`/`c` flag to get it fixed automatically:

```pre
Found 3 crates with unformatted features:
  polkadot-cli
  polkadot-parachain
  polkadot-core-primitives
  polkadot-primitives
  ...
Formatted 37 crates (all fixed).
```

Looking at the diff that this command produces; Zepter assumes a default line width of 80. For one-lined features they will just be padded with spaces:

```patch
-default = [
-       "static_assertions",
-]
+default = [ "static_assertions" ]
```

Entries are sorted, comments are kept and indentation is one tab for your convenience 😊

```patch
-       # Hi
-       "xcm/std",
        "xcm-builder/std",
+       # Hi
+       "xcm/std",
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

The auto-fix can be configured to enable specific optional dependencies as non-optional via `--feature-enables-dep="runtime-benchmarks:frame-benchmarking"` for example. In this case the `frame-benchmarking` dependency would enabled as non-optional if the `runtime-benchmarks` feature is enabled.

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

## CI Usage

Zepter is currently being used experimentially in the [Substrate](https://github.com/paritytech/substrate/blob/19971bd3eafa6394d918030f4142f85ea54404c0/scripts/ci/gitlab/pipeline/check.yml#L56-L60) CI to spot missing features. Usage in the Polkadot repository will be added soon as well.  
When these two experiments proove the usefulness and reliability of Zepter for CI application, then a more streamlined process will be introduced (possibly in the form of CI actions).

## Testing

UI and integration tests are run with the normal `cargo test`.  
Environment overwrites exist for:
- `OVERWRITE`: Update the `cout` and `diff` locks.
- `UI_FILTER`: Regex to selectively run files.
- `KEEP_GOING`: Print `FAILED` but don't abort. TODO: It's buggy

## Planned Features

- [x] Add feature information to the enabled deps
- [x] Optimize `shortest_path` function
- [ ] Add support for config files
- [ ] Feature sorting and deduplication

<!-- LINKS -->
[Cumulus]: https://github.com/paritytech/cumulus
[Substrate]: https://github.com/paritytech/substrate
