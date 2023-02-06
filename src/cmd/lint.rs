use crate::CrateId;
use cargo_metadata::{Metadata, PackageId};

use std::collections::{BTreeMap, BTreeSet};

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

	#[clap(long, required = true)]
	pub features: Vec<String>,

	/// Show crate versions.
	#[clap(long)]
	crate_versions: bool,
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
		log::info!("Using manifest: {:?}", self.tree_args.manifest_path);
		let feature = self.features.first().unwrap().clone();
		let meta = self.metadata_of().expect("Loads metadata");
		let pkgs = meta.packages.iter().collect::<Vec<_>>();
		if let Some(root) = meta.root_package() {
			println!("Analyzing {root:?}");
		} else {
			println!("Analyzing workspace");
		}
		let lookup = |id: &str| {
			let id = PackageId { repr: id.to_string() }; // TODO optimize
			pkgs.iter()
				.find(|pkg| pkg.id == id)
				.unwrap_or_else(|| panic!("Could not find crate {id} in the metadata"))
		};

		// (Crate that is not forwarding the feature) -> (Dependency that it is not forwarded to)
		let mut propagate_missing = BTreeMap::<CrateId, BTreeSet<CrateId>>::new();
		// (Crate that missing the feature) -> (Dependency that has it)
		let mut feature_missing = BTreeMap::<CrateId, BTreeSet<CrateId>>::new();
		// Crate that has the feature but does not need it.
		let mut feature_maybe_unused = BTreeSet::<CrateId>::new();

		for pkg in pkgs.iter() {
			let mut feature_used = false;
			// TODO that it does not enable other features.

			for dep in pkg.dependencies.iter() {
				// TODO handle default features.
				// Resolve the dep according to the metadata.
				let resolved = if self.tree_args.workspace {
					// TODO horrible code
					meta.workspace_members
						.iter()
						.find(|id| id.to_string().starts_with(format!("{} ", dep.name).as_str()))
						.map(|id| lookup(id.to_string().as_str()))
				} else {
					meta.resolve
						.as_ref()
						.and_then(|resolve| {
							resolve.nodes.iter().find(|node| {
								node.id.to_string().starts_with(format!("{} ", dep.name).as_str())
							})
						})
						.map(|node| lookup(node.id.to_string().as_str()))
				};

				let Some(dep) = resolved else {
					assert!(meta.workspace_members.iter().find(|id| id.to_string().starts_with(format!("{} ", dep.name).as_str())).map(|id| lookup(id.to_string().as_str())).is_none(), "Impossible resolve must not resolve to a workspace member.");
					// Either outside workspace or not resolved, possibly due to not being used at all because of the target or whatever.
					feature_used = true;
					continue;
				};

				if dep.features.contains_key(&feature) {
					match pkg.features.get(&feature) {
						None => {
							feature_missing
								.entry(pkg.id.to_string())
								.or_default()
								.insert(dep.id.to_string());
						},
						Some(enabled) => {
							if !enabled.contains(&format!("{}/{}", dep.name, feature)) {
								propagate_missing
									.entry(pkg.id.to_string())
									.or_default()
									.insert(dep.id.to_string());
							} else {
								// All ok
								feature_used = true;
							}
						},
					}
				}
			}

			if !feature_used && pkg.features.contains_key(&feature) {
				feature_maybe_unused.insert(pkg.id.to_string());
			}
		}
		let faulty_crates: BTreeSet<CrateId> = propagate_missing
			.keys()
			.chain(feature_missing.keys())
			.chain(feature_maybe_unused.iter())
			.cloned()
			.collect();

		let (mut errors, mut warnings) = (0, 0);
		for krate in faulty_crates {
			println!("crate {:?}\n  feature {:?}", lookup(&krate).name, feature);

			// join
			if let Some(deps) = feature_missing.get(&krate) {
				let joined = deps
					.iter()
					.map(|d| lookup(d))
					.map(|dep| dep.name.to_string())
					.collect::<Vec<_>>()
					.join("\n      ");
				println!(
					"    must exit because {} dependencies have it:\n      {}",
					deps.len(),
					joined
				);
				errors += 1;
			}
			if let Some(deps) = propagate_missing.get(&krate) {
				let joined = deps
					.iter()
					.map(|d| lookup(d))
					.map(|dep| dep.name.to_string())
					.collect::<Vec<_>>()
					.join("\n      ");
				println!("    must propagate to:\n      {joined}");
				errors += 1;
			}
			if let Some(_dep) = feature_maybe_unused.get(&krate) {
				if !feature_missing.contains_key(&krate) && !propagate_missing.contains_key(&krate)
				{
					println!("    is not used by any dependencies");
					warnings += 1;
				}
			}
		}
		if errors > 0 || warnings > 0 {
			println!("Generated {errors} errors and {warnings} warnings");
		}
	}

	fn metadata_of(&self) -> cargo_metadata::Result<Metadata> {
		let mut cmd = cargo_metadata::MetadataCommand::new();
		cmd.manifest_path(&self.tree_args.manifest_path);
		if self.tree_args.workspace {
			cmd.no_deps();
		}
		log::info!("Running cargo metadata: {:?}", cmd.cargo_command());
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
			let dont_have = (*dont_have).clone().strip_features();

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
