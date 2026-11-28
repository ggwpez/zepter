// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod never_implies;
mod only_enables;

use super::build_feature_dag;
use crate::{
	cmd::{resolve_dep, CargoArgs, CommandError, GlobalArgs, RenamedPackage},
	log, CrateId,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, clap::Parser)]
pub struct WhyEnabledCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,

	#[clap(long, short)]
	package: String,

	#[clap(long)]
	feature: String,
}

#[derive(Debug, clap::Parser)]
pub struct OnlyEnablesCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,

	#[clap(long)]
	precondition: String,

	#[clap(long)]
	only_enables: String,
}

#[derive(Debug, clap::Parser)]
pub struct NeverEnablesCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,

	/// The left side of the feature implication.
	///
	/// Can be set to `default` for the default feature set.
	#[clap(long, required = true)]
	precondition: String,

	/// The right side of the feature implication.
	///
	/// If [precondition] is enabled, this stays disabled.
	#[clap(long, required = true)]
	stays_disabled: String,
}

#[derive(Debug, clap::Parser)]
pub struct NeverImpliesCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,

	/// The left side of the feature implication.
	///
	/// Can be set to `default` for the default feature set.
	#[clap(long, required = true)]
	precondition: String,

	/// The right side of the feature implication.
	///
	/// If [precondition] is enabled, this stays disabled.
	#[clap(long, required = true)]
	stays_disabled: String,

	/// Show the source location of crates in the output.
	#[clap(long)]
	show_source: bool,

	/// Show the version of the crates in the output.
	#[clap(long)]
	show_version: bool,

	/// Delimiter for rendering dependency paths.
	#[clap(long, default_value = " -> ")]
	path_delimiter: String,
}

impl NeverEnablesCmd {
	pub fn run(&self, _global: &GlobalArgs) -> Result<(), String> {
		let meta = self.cargo_args.load_metadata()?;
		log::info!(
			target: super::LOG_TARGET,
			"Checking that feature {:?} never enables {:?}",
			self.precondition,
			self.stays_disabled
		);
		let pkgs = &meta.packages;
		// (Crate -> dependencies) that invalidate the assumption.
		let mut offenders = BTreeMap::<CrateId, BTreeSet<RenamedPackage>>::new();

		for lhs in pkgs.iter() {
			let Some(enabled) = lhs.features.get(&self.precondition) else { continue };

			// TODO do the same in other command.
			if enabled.contains(&self.stays_disabled) {
				offenders.entry(lhs.id.to_string()).or_default().insert(RenamedPackage::new(
					(*lhs).clone(),
					None,
					false,
				)); // TODO
			} else {
				log::info!(
					target: super::LOG_TARGET,
					"Feature {:?} not enabled on crate {:?}: {enabled:?}",
					self.stays_disabled,
					lhs.id
				);
			}

			for rhs in lhs.dependencies.iter() {
				let Some(rhs) = resolve_dep(lhs, rhs, &meta) else { continue };

				if enabled.contains(&format!("{}/{}", rhs.name(), self.stays_disabled)) {
					offenders.entry(lhs.id.to_string()).or_default().insert(rhs);
				}
			}
		}

		for (lhs, rhss) in offenders {
			// Find by id
			let lhs = pkgs
				.iter()
				.find(|p| p.id.to_string() == lhs)
				.ok_or_else(|| format!("Could not find crate '{lhs}' in the metadata."))?;
			println!("crate {:?}\n  feature {:?}", lhs.name, self.precondition);
			// TODO support multiple left/right side features.
			println!("    enables feature {:?} on dependencies:", self.stays_disabled);

			for rhs in rhss {
				match &rhs.rename {
					Some(_) => {
						println!("      {} (renamed from {})", rhs.pkg.name, rhs.name());
					},
					None => {
						println!("      {}", rhs.name());
					},
				}
			}
		}

		Ok(())
	}
}

impl WhyEnabledCmd {
	pub fn run(&self, _global: &GlobalArgs) -> Result<(), CommandError> {
		let meta = self.cargo_args.load_metadata()?;
		let dag = build_feature_dag(&meta, &meta.packages)?;
		let pkgs = meta.packages;
		let mut found_crate_and_feature = false;
		let mut found_crate = false;
		let mut enabled_by = vec![];

		let lookup = |id: &str| pkgs.iter().find(|pkg| pkg.id.to_string() == id);

		for (lhs, rhs) in dag.edges.iter() {
			for rhs in rhs.iter() {
				// A bit ghetto, but i don't want to loose unresolved rhs crates.
				let resolved = lookup(&rhs.0).map(|r| r.name.to_string()).unwrap_or(rhs.0.clone());
				if resolved == self.package {
					found_crate = true;
				}

				if resolved == self.package && rhs.1 == self.feature {
					let lhs_resolved = lookup(&lhs.0).ok_or_else(|| {
						format!("Could not find crate '{}' in the metadata", lhs.0)
					})?;
					found_crate_and_feature = true;
					enabled_by.push((lhs_resolved.name.clone(), lhs.1.clone()));
				}
			}
		}

		if !found_crate {
			println!("Did not find package {} on the rhs of the dependency tree", self.package);
			return Err(CommandError::reported(1))
		}
		if !found_crate_and_feature {
			// TODO find edit distance to the closest one
			println!("Package {} does not have feature {}", self.package, self.feature);
			return Err(CommandError::reported(1))
		}
		debug_assert!(!enabled_by.is_empty());
		println!("Feature {}/{} is enabled by:", self.feature, self.package);
		for (name, feature) in enabled_by {
			println!("  {name}/{feature}");
		}

		Ok(())
	}
}
