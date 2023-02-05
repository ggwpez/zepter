use crate::Crate;
use std::collections::{BTreeSet, BTreeMap};
use crate::dag::{Path, Dag};
use serde_json::Value;
use cargo_metadata::Metadata;

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

		

	}

	fn metadata_of(&self) -> cargo_metadata::Result<Metadata> {
		let mut cmd = cargo_metadata::MetadataCommand::new();
		cmd.manifest_path(&self.tree_args.manifest_path);
		cmd.exec()
	}

	/*pub fn run(&self) {
		let dag = super::common::tree(&self.tree_args);
		let feature = self.tree_args.features.first().unwrap().clone();

		// Find all crates that have the feature.
		let have: BTreeSet<&Crate> = dag.edges.iter().map(|(krate, _)| krate).filter(|krate| krate.enabled_features.contains(&feature)||krate.has_features.contains(&feature)).collect();
		// Factor out the enabled features.
		//let have: BTreeSet<Crate> = have.into_iter().cloned().map(|krate| krate.without_enabled_features()).collect();
		/*log::info!("Found {} crates with the feature", have.len());
		let mut connected = 0;
		let mut errors = BTreeMap::new();

		// Try to find a path between two `have`.
		for from in have.iter() {
			for to in have.iter() {
				if from == to {
					continue;
				}

				if let Some(mut path) = dag.any_path(&from, &to) {
					path = path.into_compact();
					if let Err((from, to)) = self.check_path(&feature, &path, &dag) {
						errors.entry(from).or_insert_with(BTreeSet::new).insert(to);
					}
					connected += 1;
				}
			}
		}
		log::info!("Found {} connected pairs", connected);
		for (from, tos) in errors {
			println!("{} is not passing {:?} to {} dependencies", from, feature, tos.len());
			for to in tos {
				println!("  {}", to);
			}
		}*/

		let dont_have: BTreeSet<&Crate> = dag.edges.iter().map(|(krate, _)| krate).filter(|krate| !krate.enabled_features.contains(&feature)&&!krate.has_features.contains(&feature)).collect();
		// Check if any of these crates have a direct connection to a `have`.
		let mut errors = BTreeMap::new();

		for dont_have in dont_have.into_iter() {
			let dont_have = (*dont_have).clone().without_features();
			
			for mut have in have.iter() {
				//have = have.without_enabled_features();

				if dag.connected(&dont_have, &have) {
					errors.entry(dont_have.clone()).or_insert_with(BTreeSet::new).insert(have.clone());
				}
			}
		}
		for (from, tos) in errors {
			println!("{} is not propagating {:?} to", from, feature);
			for to in tos {
				println!("  {}", to);
			}
		}
	}

	fn check_path<'a>(&self, feature: &String, path: &Path<'a, Crate>, dag: &'a Dag<Crate>) -> Result<(), (Crate, Crate)> {
		for i in 1..path.0.len() {
			if !path.0[i].has_features.contains(feature) {
				return Err((path.0[i-1].clone().into_owned(), path.0[i].clone().into_owned()));
			}
		}
		Ok(())
	}*/
}
