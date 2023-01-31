# Features

Check why and how a dependency in your workspace gets enabled. This is useful in cases where you encounter weird build errors which are caused by unintentional inclusion of dependencies or features.

## Install

```bash
cargo install feature
```

## Examples

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
