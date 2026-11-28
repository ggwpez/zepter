// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::GlobalArgs;
use crate::cmd::lint::build_feature_dag;
#[cfg(feature = "debugging")]
use crate::{cmd::lint::CrateAndFeature, prelude::Dag};

use cargo_metadata::Metadata;
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
	pub fn run(&self, g: &GlobalArgs) -> Result<(), String> {
		g.warn_unstable();
		let meta = self.cargo_args.load_metadata()?;
		let dag = build_feature_dag(&meta, &meta.packages)?;

		if !self.no_root {
			println!("Root: {}", meta.workspace_root);
		}
		println!("Num workspace members: {}", meta.workspace_members.len());
		println!("Num dependencies: {}", meta.packages.len());
		println!("DAG nodes: {}, links: {}", dag.direct.num_nodes(), dag.direct.num_edges());

		#[cfg(feature = "debugging")]
		self.connectivity_buckets(&dag.direct);

		if !self.no_benchmark {
			let (took, points) = Self::measure(&meta)?;
			println!("DAG setup time: {took:.2?} (avg from {points} runs)");
		}

		Ok(())
	}

	#[cfg(feature = "debugging")]
	pub fn connectivity_buckets(&self, dag: &Dag<CrateAndFeature>) {
		let mut histogram = histo::Histogram::with_buckets(10);

		for node in dag.lhs_nodes() {
			histogram.add(dag.degree(node) as u64);
		}

		println!("{histogram}");
	}

	fn measure(meta: &Metadata) -> Result<(Duration, u32), String> {
		// Run at least: 10 times or 5 secs, whatever takes longer.
		let mut took = Duration::default();
		let mut count = 0;

		while took < Duration::from_secs(1) || count < 10 {
			took += Self::measure_once(meta)?;
			count += 1;
		}

		Ok((took / count, count))
	}

	fn measure_once(meta: &Metadata) -> Result<Duration, String> {
		let start = Instant::now();
		let _ = build_feature_dag(meta, &meta.packages)?;
		Ok(start.elapsed())
	}
}
