// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand::{Rng, SeedableRng};
use zepter::prelude::*;

fn build_dag(nodes: usize, edges: usize) -> Dag<usize> {
	let mut rng = rand::rngs::StdRng::seed_from_u64(42);

	let mut dag = Dag::default();
	for i in 0..nodes {
		dag.add_node(i);
	}
	for _ in 0..edges {
		let from = rng.gen_range(0..nodes);
		let to = rng.gen_range(0..nodes);
		dag.add_edge(from, to);
	}
	dag
}

fn any_path(dag: &Dag<usize>) {
	dag.any_path(&0, &1);
}

fn criterion_benchmark(c: &mut Criterion) {
	let dag = build_dag(1000, 1000);
	c.bench_function("DAG 1k/1k", |b| b.iter(|| black_box(any_path(&dag))));
	let dag = build_dag(1000, 5000);
	c.bench_function("DAG 1k/5k", |b| b.iter(|| black_box(any_path(&dag))));
	let dag = build_dag(10000, 1000);
	c.bench_function("DAG 10k/1k", |b| b.iter(|| black_box(any_path(&dag))));
	let dag = build_dag(10000, 50000);
	c.bench_function("DAG 10k/50k", |b| b.iter(|| black_box(any_path(&dag))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
