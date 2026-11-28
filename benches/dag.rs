// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use criterion::{criterion_group, criterion_main, Criterion};
use rand::{RngExt, SeedableRng};
use std::hint::black_box;
use zepter::{cmd::lint::build_feature_dag, prelude::*};

fn build_dag(nodes: usize, edges: usize) -> Dag<usize> {
	let mut rng = rand::rngs::StdRng::seed_from_u64(42);

	let mut dag = Dag::default();
	for i in 0..nodes {
		dag.add_node(i);
	}
	for _ in 0..edges {
		let from = rng.random_range(0..nodes);
		let to = rng.random_range(0..nodes);
		dag.add_edge(from, to);
	}
	dag
}

fn any_path(dag: &Dag<usize>) {
	dag.any_path(&0, &1);
}

fn dag(c: &mut Criterion) {
	let dag = build_dag(1000, 1000);
	c.bench_function("DAG 1k/1k", |b| {
		b.iter(|| {
			any_path(&dag);
			black_box(());
		});
	});
	let dag = build_dag(1000, 5000);
	c.bench_function("DAG 1k/5k", |b| {
		b.iter(|| {
			any_path(&dag);
			black_box(());
		});
	});
	let dag = build_dag(10000, 1000);
	c.bench_function("DAG 10k/1k", |b| {
		b.iter(|| {
			any_path(&dag);
			black_box(());
		});
	});
	let dag = build_dag(10000, 50000);
	c.bench_function("DAG 10k/50k", |b| {
		b.iter(|| {
			any_path(&dag);
			black_box(());
		});
	});
}

fn polkadot_sdk(c: &mut Criterion) {
	let path = std::env::var("META_JSON_PATH").unwrap_or("meta.json".into());
	let path = std::fs::canonicalize(path).unwrap();
	let file = std::fs::read_to_string(path).unwrap();
	let meta = serde_json::from_str::<cargo_metadata::Metadata>(&file).unwrap();

	let pkgs = &meta.packages;
	let dag = build_feature_dag(&meta, pkgs).expect("valid benchmark metadata");

	c.bench_function("Polkadot-SDK / DAG / setup", |b| {
		b.iter(|| {
			let dag = build_feature_dag(&meta, pkgs).expect("valid benchmark metadata");
			black_box(dag)
		});
	});

	let from = dag.direct.lhs_iter().find(|c| c.0.starts_with("kitchensink-runtime ")).unwrap();
	let to = dag.direct.rhs_iter().find(|c| c.0.starts_with("sp-io ")).unwrap();

	c.bench_function("Polkadot-SDK / DAG / reachability: false", |b| {
		b.iter(|| {
			let p = dag.implied_path(from, |node| node == to);
			black_box(p)
		});
	});
}

criterion_group!(benches, polkadot_sdk, dag);
criterion_main!(benches);
