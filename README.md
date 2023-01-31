# Features

Check why and how a dependency in your workspace gets enabled. This is useful in cases where you encounter weird build errors which are caused by unintentional inclusion of dependencies or features.

## Install

```bash
cargo install feature
```

## Examples

Using [substrate](https://github.com/paritytech/substrate) as example to find out how `node-cli` depends on `pallet-proxy`:

```bash
feature trace --manifest-path substrate/Cargo.toml node-cli snow
```

output:

```pre
[INFO  feature] Using manifest "../substrate/Cargo.toml"
[INFO  feature] Calculating shortest path from 'node-cli' to 'snow'
node-cli -> try-runtime-cli -> substrate-rpc-client -> sc-rpc-api -> sc-chain-spec -> sc-telemetry -> libp2p -> libp2p-webrtc -> libp2p-noise -> snow
```

## Roadmap

- [ ] Add feature information to the enabled deps
- [ ] Allow manual skipping of dev dependencies (currently always skipped)
- [ ] Introduce filters for versions and features for argument `to`
- [ ] Optimize `shortest_path` function
- [ ] Create lint rules which can be used to validate that certain constraints in the work-space hold
