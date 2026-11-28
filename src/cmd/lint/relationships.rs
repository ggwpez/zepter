// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod never_implies;
mod only_enables;

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
		let mut found_crate = false;
		let mut enabled_by = BTreeSet::new();
		for pkg in &meta.packages {
			// A dependency-table request is not a feature on the parent. Keep
			// the declaration's kind, alias and target in its explanation.
			for dep in pkg.dependencies.iter().filter(|dep| dep.name == self.package) {
				found_crate = true;
				if (self.feature == "default" && dep.uses_default_features) ||
					dep.features.contains(&self.feature)
				{
					let kind = crate::kind_to_str(&dep.kind);
					let table = match &dep.target {
						Some(target) => format!("target.'{target}'.{kind}"),
						None => kind.to_owned(),
					};
					let alias = dep.rename.as_deref().unwrap_or(&dep.name);
					enabled_by.insert(format!(
						"{} requests {alias}/{} in [{table}]",
						pkg.name, self.feature
					));
				}
			}
			for (feature, entries) in &pkg.features {
				for entry in entries {
					if let Some((name, target)) = entry.split_once('/') {
						let alias = name.trim_end_matches('?');
						if target == self.feature &&
							pkg.dependencies.iter().any(|dep| {
								dep.name == self.package &&
									dep.rename.as_deref().unwrap_or(&dep.name) == alias
							}) {
							let always_active = name == alias ||
								super::feature_activates_dependency(pkg, feature, alias);
							let reason = if always_active {
								format!("{}/{feature}", pkg.name)
							} else {
								format!("{}/{feature} (if {alias} is active)", pkg.name)
							};
							enabled_by.insert(reason);
						}
					} else if pkg.name == self.package && !entry.starts_with("dep:") {
						found_crate = true;
						if entry == &self.feature {
							enabled_by.insert(format!("{}/{feature}", pkg.name));
						}
					}
				}
			}
		}

		if !found_crate {
			println!("Did not find package {} on the rhs of the dependency tree", self.package);
			return Err(CommandError::reported(1))
		}
		if enabled_by.is_empty() {
			// TODO find edit distance to the closest one
			println!("Package {} does not have feature {}", self.package, self.feature);
			return Err(CommandError::reported(1))
		}
		println!("Feature {}/{} is enabled by:", self.feature, self.package);
		for reason in enabled_by {
			println!("  {reason}");
		}

		Ok(())
	}
}
