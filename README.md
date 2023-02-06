# Features

Check why and how a dependency in your workspace gets enabled. This is useful in cases where you encounter weird build errors which are caused by unintentional inclusion of dependencies or features.

## Install

```bash
cargo install feature
```

## Example - Feature Propagation Lint

Let's check that the `runtime-benchmarks` feature is properly passed down to all the dependencies of all crates in the workspace of [Cumulus]:  

```bash
feature lint propagate-feature --manifest-path ../cumulus/Cargo.toml --feature runtime-benchmarks --workspace
```

The output reveals that there are a lot of crates that violate this assumption:  

```pre
crate "asset-test-utils"
  feature "runtime-benchmarks"
    must exit because 1 dependencies have it:
      pallet-collator-selection
crate "bridge-hub-kusama-runtime"
  feature "runtime-benchmarks"
    must propagate to:
      cumulus-pallet-parachain-system
...
Generated 24 errors
```

Without the `--workspace` it even detects 243 violations. Automatic fixing will be helpful here (TBD).

Now you can verify this for the [bridge-hub-kusama-runtime](https://github.com/paritytech/cumulus/blob/f754f03e550666e9124e7dc5cade20d20abc99d4/parachains/runtimes/bridge-hubs/bridge-hub-kusama/Cargo.toml#L143) which is indeed missing the feature for `cumulus-pallet-parachain-system` while that is clearly [providing](https://github.com/paritytech/cumulus/blob/f754f03e550666e9124e7dc5cade20d20abc99d4/pallets/parachain-system/Cargo.toml#L78) this feature 🤔. There will probably be some false-positive/negatives currently, since I did not properly test it yet.

## Example - Dependency tracing

Recently there was a build error in the [Substrate](https://github.com/paritytech/substrate) master CI which was caused by a downstream dependency [`snow`](https://github.com/mcginty/snow/issues/146). To investigate this, it is useful to see *how* Substrate depends on it.  

Let's find out how `node-cli` depends on `snow`:

```bash
feature trace --manifest-path substrate/Cargo.toml node-cli snow
```

output:

```
node-cli -> try-runtime-cli -> substrate-rpc-client -> sc-rpc-api ->sc-chain-spec -> sc-telemetry -> libp2p -> libp2p-webrtc -> libp2p-noise -> snow
```

So it comes from libp2p, okay. Good to know.

## Roadmap

- [ ] Add feature information to the enabled deps
- [ ] Allow manual skipping of dev dependencies (currently always skipped)
- [ ] Introduce filters for versions and features for argument `to`
- [ ] Optimize `shortest_path` function
- [ ] Create lint rules which can be used to validate that certain constraints in the work-space hold

<!-- LINKS -->
[Cumulus]: https://github.com/paritytech/cumulus
