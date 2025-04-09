# Zepter

[![Rust](https://github.com/ggwpez/zepter/actions/workflows/rust.yml/badge.svg)](https://github.com/ggwpez/zepter/actions/workflows/rust.yml)
[![crates.io](https://img.shields.io/crates/v/zepter.svg)](https://crates.io/crates/zepter)
![MSRV](https://img.shields.io/badge/MSRV-1.78-informational)
[![docs.rs](https://img.shields.io/docsrs/zepter)](https://docs.rs/zepter/latest/zepter)

Analyze, Fix and Format features in your Rust workspace. The goal of this tool is to have this CI ready to prevent common errors with Rust features.

## Install

```sh
cargo install zepter -f --locked
```

## Commands

zepter
- : this is the same as `run`.
- run: Run a workflow from the config file. Uses `default` if none is specified.
- format
  - features: Format features layout and remove duplicates.
- trace: Trace dependencies paths.
- lint
  - propagate-features: Check that features are passed down.
  - never-enables: A feature should never enable another other.
  - never-implies *(⚠️ unstable)*: A feature should never transitively imply another one.
  - only-enables *(⚠️ unstable)*: A features should exclusively enable another one.
  - why-enables *(⚠️ unstable)*: Find out why a specific feature is enables.
- debug: *(⚠️ unstable)* just for quick debugging some stuff.
- transpose *(⚠️ unstable)*
  - dependency
    - lift-to-workspace: Lifts crate dependencies to the workspace.

## Example - Using Workspace dependencies

Currently this only works for external dependencies and has some cases where it does not work. However, all the changes
that it *does* do, should be correct.

You can see this in action for example [here](https://github.com/paritytech/polkadot-sdk/pull/3366) or try it out yourself.
For example, pulling up all `serde*` crates to the workspace can look like this:

```bash
zepter transpose dependency lift-to-workspace "regex:^serde.*" --ignore-errors
```

It will probably print that some versions are not aligned. Zepter has the default behaviour to be cautious to not accidentally
update some dependencies by pulling them up. To get around this and actually do the changes, you can do:

```bash
zepter transpose dependency lift-to-workspace "regex:^serde.*" --ignore-errors --fix --version-resolver=highest
```

This will try to select the "highest" SemVer version of each crate.

## Example - Feature Formatting

To ensure that your features are in canonical formatting, just run:

```bash
zepter format features
# Or shorter:
zepter f f
```

The output will tell you which features are missing formatting:

```pre
Found 37 crates with unformatted features:
  polkadot-cli
  polkadot-runtime-common
  polkadot-runtime-parachains
  ...
Run again with `--fix` to format them.
```

Re-running with `--fix`/`-f`:

```pre
Found 37 crates with unformatted features:
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

## Config Files

⚠️ the syntax for workflows is highly experimental and bound to change.

The first step is that Zepter checks that it is executed in a rust workspace. Otherwise it fails directly. Then a workflow file is located as follows:

- `$WORKSPACE/zepter.yaml`
- `$WORKSPACE/.zepter.yaml`
- `$WORKSPACE/.cargo/zepter.yaml`
- `$WORKSPACE/.cargo/.zepter.yaml`
- `$WORKSPACE/.config/zepter.yaml`
- `$WORKSPACE/.config/.zepter.yaml`

It uses the first file that is found and errors if none is found. Currently it not possible to overwrite the config in a sub-folder.

### Workflows

> [!NOTE]
> A production example can be found in the [Polkadot-SDK](https://github.com/paritytech/polkadot-sdk/blob/8ebb5c3319fa52d68f2d76f90f5787a96de254be/.config/zepter.yaml) or in the [`presets`](presets/polkadot.yaml).


It is possible to aggregate the long commands into workflows instead of typing them each time. Zepter tries to locate a config file and run the `default` workflow when it is bare invoked without any arguments.  
Alternately, it is possible to use `zepter run default`, or any other workflow name.

Config files can contain workflows like this:

```yaml
workflows:
  default:
    - [ 'propagate-features', ... ]
    - ...
```

It is also possible to extend previous steps:

```yaml
workflows:
  check:
    - ...
  default:
    - [ $check.0, '--fix' ]
    - ...
```

## CI Usage

Zepter is currently being used in the [Polkadot-SDK](https://github.com/paritytech/polkadot-sdk/pull/1194) CI to spot missing features.  
When these two experiments proove the usefulness and reliability of Zepter for CI application, then a more streamlined process will be introduced (possibly in the form of CI actions).

## Testing

Unit tests: `cargo test`
UI and downstream integration tests: `cargo test -- --ignored`

Environment overwrites exist for the UI tests to:
- `OVERWRITE`: Update the UI diff locks.
- `UI_FILTER`: Regex to selectively run UI test.
- `KEEP_GOING`: Print `FAILED` but don't abort on the first failed UI test.

## Development Principles

- Compile time is human time. Compile time should *always* be substantially below 1 minute.
- Minimal external dependencies. Reduces source of errors and compile time.
- Tests. So far, the tool is used since a year extensively in CI and never got a bug report. It should stay like this.

<!-- LINKS -->
[Cumulus]: https://github.com/paritytech/cumulus
[Substrate]: https://github.com/paritytech/substrate
