use crate::Crate;
use std::collections::BTreeSet;
use crate::dag::{Path, Dag};

#[derive(Debug, clap::Parser)]
pub struct LintCmd {
	#[clap(subcommand)]
	subcommand: SubCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum SubCommand {
	PropagateFeature(PropagateFeatureCmd),
}

#[derive(Debug, clap::Parser)]
pub struct PropagateFeatureCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	tree_args: super::common::TreeArgs,
}

impl LintCmd {
	pub(crate) fn run(&self) {
		match &self.subcommand {
			SubCommand::PropagateFeature(cmd) => cmd.run(),
		}
	}
}

impl PropagateFeatureCmd {
	pub fn run(&self) {
		let dag = super::common::tree(&self.tree_args);
		let feature = self.tree_args.features.first().unwrap().clone();

		// Find all crates that have the feature.
		let have: BTreeSet<&Crate> = dag.edges.iter().map(|(krate, _)| krate).filter(|krate| krate.has_features.contains(&feature)).collect();
		// Factor out the enabled features.
		let have: BTreeSet<Crate> = have.into_iter().cloned().map(|krate| krate.without_enabled_features()).collect();
		log::info!("Found {} crates with the feature", have.len());
		let mut connected = 0;
		let mut errors = 0;

		// Try to find a path between two `have`.
		for from in have.iter() {
			for to in have.iter() {
				if from == to {
					continue;
				}

				if let Some(path) = dag.any_path(&from, &to) {
					log::info!("{} depends on {} via {}", from, to, path);
					errors += self.check_path(&feature, &path, &dag);
					connected += 1;
				}
			}
		}
		log::info!("Found {} connected pairs", connected);
		log::warn!("Generated {} warnings", errors);
	}

	fn check_path<'a>(&self, feature: &String, path: &Path<'a, Crate>, dag: &'a Dag<Crate>) -> u32 {
		let mut errors = 0;
		let first = path.0.first().unwrap();
		for krate in path.0.iter().skip(1) {
			if !krate.enabled_features.contains(feature) {
				log::warn!("'{:?}' misses feature {:?} for '{:?}'", first, feature, krate);
				errors+=1;
			}
		}
		errors
	}
}
