// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{lint::CrateAndFeature, GlobalArgs};
use crate::{cmd::lint::build_feature_dag, prelude::Dag};

use cargo_metadata::Metadata;
use histo::Histogram;
use std::time::{Duration, Instant};

#[derive(Debug, clap::Parser)]
pub struct DebugCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	#[clap(long)]
	no_benchmark: bool,

	#[clap(long)]
	no_root: bool,
}

impl DebugCmd {
	pub fn run(&self, g: &GlobalArgs) {
		g.warn_unstable();
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		let dag = build_feature_dag(&meta, &meta.packages);
		
		if !self.no_root {
			println!("Root: {}", meta.workspace_root.to_string());
		}
		println!("Num workspace members: {}", meta.workspace_members.len());
		println!("Num dependencies: {}", meta.packages.len());
		println!("DAG nodes: {}, links: {}", dag.num_nodes(), dag.num_edges());
		self.connectivity_buckets(&dag);
		if !self.no_benchmark {
			let (took, points) = Self::measure(&meta);
			println!("DAG setup time: {:.2?} (avg from {} runs)", took, points);
		}
	}

	pub fn connectivity_buckets(&self, dag: &Dag<CrateAndFeature>) {
		let mut histogram = Histogram::with_buckets(10);

		for node in dag.lhs_nodes() {
			histogram.add(dag.degree(node) as u64);
		}

		println!("{}", histogram);
	}

	fn measure(meta: &Metadata) -> (Duration, u32) {
		// Run at least: 10 times or 5 secs, whatever takes longer.
		let mut took = Duration::default();
		let mut count = 0;

		while took < Duration::from_secs(1) || count < 10 {
			took += Self::measure_once(meta);
			count += 1;
		}

		assert!(took >= Duration::from_secs(1) || count >= 10);
		(took / count, count)
	}

	fn measure_once(meta: &Metadata) -> Duration {
		let start = Instant::now();
		let _ = build_feature_dag(meta, &meta.packages);
		start.elapsed()
	}
}
