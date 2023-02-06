use crate::CrateId;
use cargo_metadata::PackageId;
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
	tree_args: super::TreeArgs,

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
		let meta = self.tree_args.load_metadata().expect("Loads metadata");
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
}
