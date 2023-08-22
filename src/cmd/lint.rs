// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Lint your feature usage by analyzing crate metadata.

use crate::{
	autofix::*,
	cmd::{parse_key_val, resolve_dep, RenamedPackage},
	grammar::*,
	log,
	prelude::*,
	CrateId,
};
use cargo_metadata::{Metadata, Package, PackageId};
use core::{
	fmt,
	fmt::{Display, Formatter},
};
use std::{
	collections::{BTreeMap, BTreeSet},
	fs::canonicalize,
	path::PathBuf,
};

use super::GlobalArgs;

/// Lint your feature usage by analyzing crate metadata.
#[derive(Debug, clap::Parser)]
pub struct LintCmd {
	#[clap(subcommand)]
	subcommand: SubCommand,
}

/// Sub-commands of the [Lint](LintCmd) command.
#[derive(Debug, clap::Subcommand)]
pub enum SubCommand {
	/// Check whether features are properly propagated.
	PropagateFeature(PropagateFeatureCmd),
	/// A specific feature never enables a specific other feature.
	NeverEnables(NeverEnablesCmd),
	/// A specific feature never implies a specific other feature.
	NeverImplies(NeverImpliesCmd),
	/// A specific feature is only implied by a specific set of other features.
	OnlyEnables(OnlyEnablesCmd),
	WhyEnabled(WhyEnabledCmd),
}

#[derive(Debug, clap::Parser)]
pub struct WhyEnabledCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	#[clap(long, short)]
	package: String,

	#[clap(long)]
	feature: String,
}

#[derive(Debug, clap::Parser)]
pub struct OnlyEnablesCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	#[clap(long)]
	precondition: String,

	#[clap(long)]
	only_enables: String,
}

#[derive(Debug, clap::Parser)]
pub struct NeverEnablesCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

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
	cargo_args: super::CargoArgs,

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

/// Verifies that rust features are properly propagated.
#[derive(Debug, clap::Parser)]
pub struct PropagateFeatureCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	/// The feature to check.
	#[clap(long, required = true)]
	feature: String,

	/// The packages to check. If empty, all packages are checked.
	#[clap(long, short, num_args(0..))]
	packages: Vec<String>,

	/// The auto-fixer will enables the feature of the dependencies as non-optional.
	///
	/// This can be used in case that a dependency should not be enabled like `dep?/feature` but
	/// like `dep/feature` instead. In this case you would pass `--feature-enables-dep
	/// feature:dep`. The option can be passed multiple times, or multiple key-value pairs can be
	/// passed at once by separating them with a comma like: `--feature-enables-dep
	/// feature:dep,feature2:dep2`. (TODO: Duplicate entries are undefined).
	#[clap(long, value_name = "FEATURE:CRATE", value_parser = parse_key_val::<String, String>, value_delimiter = ',', verbatim_doc_comment)]
	feature_enables_dep: Option<Vec<(String, String)>>,

	/// Overwrite the behaviour when the left side dependency is missing the feature.
	///
	/// This can be used to ignore missing features, treat them as warning or error. A "missing
	/// feature" here means that if `A` has a dependency `B` which has a feature `F`, and the
	/// propagation is checked then normally it would error if `A` is not forwarding `F` to `B`.
	/// Now this option modifies the behaviour if `A` does not have the feature in the first place.
	/// The default behaviour is to require `A` to also have `F`.
	#[clap(long, value_enum, value_name = "MUTE_SETTING", default_value_t = MuteSetting::Error, verbatim_doc_comment)]
	left_side_feature_missing: MuteSetting,

	/// Show crate versions in the output.
	#[clap(long)]
	show_version: bool,

	#[allow(missing_docs)]
	#[clap(flatten)]
	fixer_args: AutoFixerArgs,

	#[clap(long)]
	modify_paths: Vec<PathBuf>,

	/// Fix only issues with this package as dependency.
	#[clap(long)]
	fix_dependency: Option<String>,

	/// Fix only issues with this package as feature source.
	#[clap(long)]
	fix_package: Option<String>,
}

/// Can be used to change the default error reporting behaviour of a lint.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum MuteSetting {
	/// Ignore this behaviour.
	Ignore,
	/// Treat as error.
	Error,
}

impl LintCmd {
	pub(crate) fn run(&self, global: &GlobalArgs) {
		match &self.subcommand {
			SubCommand::PropagateFeature(cmd) => cmd.run(global),
			SubCommand::NeverEnables(cmd) => cmd.run(global),
			SubCommand::NeverImplies(cmd) => cmd.run(global),
			SubCommand::WhyEnabled(cmd) => cmd.run(global),
			SubCommand::OnlyEnables(cmd) => cmd.run(global),
		}
	}
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd, Debug)]
struct CrateAndFeature(String, String);

impl Display for CrateAndFeature {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}/{}", self.0, self.1)
	}
}

impl NeverImpliesCmd {
	pub fn run(&self, _global: &GlobalArgs) {
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		log::info!(
			"Checking that feature '{}' never implies '{}'",
			self.precondition,
			self.stays_disabled
		);
		let pkgs = &meta.packages;
		let dag = build_feature_dag(&meta, pkgs);

		for CrateAndFeature(pkg, feature) in dag.lhs_nodes() {
			let crate_and_feature = CrateAndFeature(pkg.clone(), feature.clone());
			if feature == &self.precondition {
				let Some(path) = dag
					.reachable_predicate(&crate_and_feature, |CrateAndFeature(_, enabled)| {
						enabled == &self.stays_disabled
					})
				else {
					continue
				};

				// TODO cleanup this cluster fuck
				let lookup = |id: &str| {
					pkgs.iter()
						.find(|pkg| {
							pkg.id.to_string().split(' ').next().unwrap() ==
								id.split(' ').next().unwrap()
						})
						.unwrap_or_else(|| panic!("Could not find crate '{id}' in the metadata."))
				};

				let delimiter = self.path_delimiter.replace("\\n", "\n").replace("\\t", "\t");
				let mut out = String::new();
				let mut is_first = true;

				path.for_each(|CrateAndFeature(id, feature)| {
					let krate = lookup(id);
					if !is_first {
						out.push_str(&delimiter);
					}
					is_first = false;
					out.push_str(&format!("{}/{}", krate.name, feature));
					if self.show_version {
						out.push_str(&format!(" v{}", krate.version));
					}
					if self.show_source {
						if let Some(source) = krate.source.as_ref() {
							out.push_str(&format!(" ({})", source.repr));
						} else {
							out.push_str(" (local)");
						}
					}
				});
				println!(
					"Feature '{}' implies '{}' via path:\n  {}",
					self.precondition, self.stays_disabled, out
				);

				std::process::exit(0);
			}
		}
	}
}

impl NeverEnablesCmd {
	pub fn run(&self, _global: &GlobalArgs) {
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		log::info!(
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
			if enabled.contains(&self.stays_disabled.to_string()) {
				offenders.entry(lhs.id.to_string()).or_default().insert(RenamedPackage::new(
					(*lhs).clone(),
					None,
					false,
				)); // TODO
			} else {
				log::info!(
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
			// TODO hack
			println!(
				"crate {:?}\n  feature {:?}",
				lhs.split(' ').next().unwrap(),
				self.precondition
			);
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
	}
}

impl PropagateFeatureCmd {
	pub fn run(&self, global: &GlobalArgs) {
		// Allowed dir that we can write to.
		let allowed_dir = canonicalize(&self.cargo_args.manifest_path).unwrap();
		let allowed_dir = allowed_dir.parent().unwrap();
		let feature = self.feature.clone();
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		let pkgs = meta.packages.iter().collect::<Vec<_>>();
		let mut to_check = pkgs.clone();
		if !self.packages.is_empty() {
			to_check =
				pkgs.iter().filter(|pkg| self.packages.contains(&pkg.name)).cloned().collect();
		}
		if to_check.is_empty() {
			panic!("No packages found: {:?}", self.packages);
		}

		let lookup = |id: &str| {
			let id = PackageId { repr: id.to_string() }; // TODO optimize
			pkgs.iter()
				.find(|pkg| pkg.id == id)
				.unwrap_or_else(|| panic!("Could not find crate {id} in the metadata"))
		};

		// (Crate that is not forwarding the feature) -> (Dependency that it is not forwarded to)
		let mut propagate_missing = BTreeMap::<CrateId, BTreeSet<RenamedPackage>>::new();
		// (Crate that missing the feature) -> (Dependency that has it)
		let mut feature_missing = BTreeMap::<CrateId, BTreeSet<RenamedPackage>>::new();
		// Crate that has the feature but does not need it.
		let mut feature_maybe_unused = BTreeSet::<CrateId>::new();

		for pkg in to_check.iter() {
			let mut feature_used = false;
			// TODO that it does not enable other features.

			for dep in pkg.dependencies.iter() {
				// TODO handle default features.
				// Resolve the dep according to the metadata.
				let Some(dep) = resolve_dep(pkg, dep, &meta) else {
					// Either outside workspace or not resolved, possibly due to not being used at
					// all because of the target or whatever.
					feature_used = true;
					continue
				};

				if dep.pkg.features.contains_key(&feature) {
					match pkg.features.get(&feature) {
						None =>
							if self.left_side_feature_missing == MuteSetting::Error {
								feature_missing.entry(pkg.id.to_string()).or_default().insert(dep);
							},
						Some(enabled) => {
							let want_opt = format!("{}?/{}", dep.name(), feature);
							let want_req = format!("{}/{}", dep.name(), feature);
							// TODO check that optional deps are only enabled as optional unless
							// overwritten with `--feature-enables-dep`.

							if !enabled.contains(&want_opt) && !enabled.contains(&want_req) {
								propagate_missing
									.entry(pkg.id.to_string())
									.or_default()
									.insert(dep);
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
			//.chain(feature_maybe_unused.iter())
			.cloned()
			.collect();

		let (mut errors, warnings) = (0, 0);
		let mut fixes = 0;
		for krate in faulty_crates {
			let krate = lookup(&krate);
			// check if we can modify in allowed_dir
			let krate_path = canonicalize(krate.manifest_path.clone().into_std_path_buf()).unwrap();

			let mut fixer = if self.fixer_args.enable {
				if krate_path.starts_with(allowed_dir) ||
					self.modify_paths.iter().any(|p| krate_path.starts_with(p))
				{
					Some(AutoFixer::from_manifest(&krate_path).unwrap())
				} else {
					log::info!(
						"Cargo path is outside of the workspace: {:?} not in {:?}",
						krate_path.display(),
						allowed_dir.display()
					);
					None
				}
			} else {
				None
			};
			println!("crate '{}'\n  feature '{}'", krate.name, feature);

			// join
			if let Some(deps) = feature_missing.get(&krate.id.to_string()) {
				let joined =
					deps.iter().map(|dep| dep.display_name()).collect::<Vec<_>>().join("\n      ");
				println!(
					"    is required by {} dependenc{}:\n      {}",
					deps.len(),
					if deps.len() == 1 { "y" } else { "ies" },
					joined
				);
				errors += deps.len();
			}
			if let Some(deps) = propagate_missing.get(&krate.id.to_string()) {
				let joined =
					deps.iter().map(|dep| dep.display_name()).collect::<Vec<_>>().join("\n      ");
				println!("    must propagate to:\n      {joined}");

				if self.fixer_args.enable &&
					self.fix_package.as_ref().map_or(true, |p| p == &krate.name)
				{
					for dep in deps {
						let dep_name = dep.name();
						if !self.fix_dependency.as_ref().map_or(true, |d| d == &dep_name) {
							continue
						}
						let Some(fixer) = fixer.as_mut() else { continue };
						let non_optional = self
							.feature_enables_dep
							.as_ref()
							.map_or(false, |v| v.contains(&(feature.clone(), dep_name.clone())));
						let opt = if !non_optional && dep.optional { "?" } else { "" };

						fixer
							.add_to_feature(
								&feature,
								format!("{}{}/{}", dep_name, opt, feature).as_str(),
							)
							.unwrap();
						log::info!("Added '{dep_name}/{feature}' to '{}'", krate.name);
						fixes += 1;
					}
				}
				errors += deps.len();
			}
			if let Some(fixer) = fixer.as_mut() {
				if fixes > 0 {
					fixer.save().unwrap();
				}
			}

			//if let Some(_dep) = feature_maybe_unused.get(&krate.id.to_string()) {
			//	if !feature_missing.contains_key(&krate.id.to_string()) &&
			// !propagate_missing.contains_key(&krate.id.to_string()) 	{
			//		println!("    is not used by any dependencies");
			//		warnings += 1;
			//	}
			//}
		}
		if let Some(e) = error_stats(errors, warnings, fixes, self.fixer_args.enable, global) {
			println!("{}", e);
		}

		if errors > fixes {
			std::process::exit(global.error_code());
		}
	}
}

fn error_stats(
	errors: usize,
	warnings: usize,
	fixes: usize,
	fix: bool,
	global: &GlobalArgs,
) -> Option<String> {
	if errors + warnings + fixes == 0 {
		return None
	}

	let mut ret: String = "Found ".into();
	if errors > 0 {
		let issues = format!("{} issue{}", errors, plural(errors));
		ret.push_str(&global.red(&issues));
	}
	if warnings > 0 {
		let warn = format!(", {} warning{}", warnings, plural(warnings));
		ret.push_str(&global.yellow(&warn));
	}
	if fix {
		if warnings + errors > 0 {
			ret.push_str(" and");
		}
		let fixed = format!(" fixed {}", fixes);
		if fixes > 0 {
			ret.push_str(&global.green(&fixed));
		} else {
			ret.push_str(&fixed);
		}

		if fixes < errors {
			let could_not = format!(" ({} could not be fixed)", errors - fixes);
			ret.push_str(&global.red(&could_not));
		}
	} else {
		ret.push_str(" (run with --fix to fix)");
	}
	Some(format!("{}.", ret))
}

impl OnlyEnablesCmd {
	pub fn run(&self, _global: &GlobalArgs) {
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		let pkgs = &meta.packages;

		for pkg in pkgs.iter() {
			for dep in pkg.dependencies.iter() {
				let Some(dep) = resolve_dep(pkg, dep, &meta) else { continue };
				if !dep.pkg.features.contains_key(&self.only_enables) {
					continue
				}

				for (feat, imply) in pkg.features.iter() {
					if feat == &self.precondition {
						continue
					} else {
						log::info!("{}: {}", feat, imply.join(", "));
					}

					let opt = if dep.optional { "?" } else { "" };
					let bad_opt = format!("{}{}/{}", dep.name(), opt, self.only_enables);
					let bad = format!("{}/{}", dep.name(), self.only_enables);
					if imply.contains(&bad) || imply.contains(&bad_opt) {
						println!(
							"{}/{} enables {}/{}",
							pkg.name,
							feat,
							dep.name(),
							self.only_enables
						);
					}
				}
			}
		}
	}
}

impl WhyEnabledCmd {
	pub fn run(&self, _global: &GlobalArgs) {
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		let dag = build_feature_dag(&meta, &meta.packages);
		let pkgs = meta.packages;
		let mut found_crate_and_feature = false;
		let mut found_crate = false;
		let mut enabled_by = vec![];

		let lookup = |id: &str| pkgs.iter().find(|pkg| pkg.id.to_string() == id);

		for (lhs, rhs) in dag.edges.iter() {
			for rhs in rhs.iter() {
				// A bit ghetto, but i don't want to loose unresolved rhs crates.
				let resolved = lookup(&rhs.0).map(|r| r.name.clone()).unwrap_or(rhs.0.clone());
				if resolved == self.package {
					found_crate = true;
				}

				if resolved == self.package && rhs.1 == self.feature {
					let lhs_resolved = lookup(&lhs.0).unwrap();
					found_crate_and_feature = true;
					enabled_by.push((lhs_resolved.name.clone(), lhs.1.clone()));
				}
			}
		}

		if !found_crate {
			println!("Did not find package {} on the rhs of the dependency tree", self.package);
			std::process::exit(1);
		}
		if !found_crate_and_feature {
			// TODO find edit distance to the closest one
			println!("Package {} does not have feature {}", self.package, self.feature);
			std::process::exit(1);
		}
		assert!(!enabled_by.is_empty());
		println!("Feature {}/{} is enabled by:", self.feature, self.package);
		for (name, feature) in enabled_by {
			println!("  {}/{}", name, feature);
		}
	}
}

fn build_feature_dag(meta: &Metadata, pkgs: &[Package]) -> Dag<CrateAndFeature> {
	let mut dag = Dag::new();

	for pkg in pkgs.iter() {
		for dep in &pkg.dependencies {
			if dep.uses_default_features {
				dag.add_edge(
					CrateAndFeature(pkg.id.to_string(), "default".into()),
					CrateAndFeature(dep.name.clone(), "default".into()),
				);
			}
			for feature in &dep.features {
				dag.add_edge(
					CrateAndFeature(pkg.id.to_string(), "default".into()),
					CrateAndFeature(dep.name.clone(), feature.into()),
				);
			}
		}

		for (feature, deps) in pkg.features.iter() {
			for dep in deps {
				if dep.contains(':') {
					let mut splits = dep.split(':');
					let dep = splits.nth(1).unwrap();
					let dep_feature = "default";
					//log::info!("Resolving '{}' as dependency of {}: {:?}", dep, pkg.name,
					// pkg.dependencies.iter().find(|d| d.name == dep));
					let dep = pkg
						.dependencies
						.iter()
						.find(|d| d.rename.clone().unwrap_or(d.name.clone()) == dep)
						.unwrap();

					let dep_id = match resolve_dep(pkg, dep, meta) {
						None => {
							// This can happen for optional dependencies who are not enabled, or
							// a weird `target` is specified or it is a dev dependency.
							// In this case we just go by name. It is a dead-end anyway.
							dep.name.clone()
						},
						Some(dep) => dep.pkg.id.to_string(), // TODO rename
					};
					dag.add_edge(
						CrateAndFeature(pkg.id.to_string(), feature.clone()),
						CrateAndFeature(dep_id.clone(), dep_feature.into()),
					);
				} else if dep.contains('/') {
					let mut splits = dep.split('/');
					let dep = splits.next().unwrap().replace('?', "");
					let dep_feature = splits.next().unwrap();

					let dep = pkg
						.dependencies
						.iter()
						.find(|d| d.rename.clone().unwrap_or(d.name.clone()) == dep)
						.unwrap_or_else(|| {
							panic!("Could not resolve dep {} of {}", dep, pkg.id.to_string())
						});

					let dep_id = match resolve_dep(pkg, dep, meta) {
						None => {
							// This can happen for optional dependencies who are not enabled, or
							// a weird `target` is specified or it is a dev dependency.
							// In this case we just go by name. It is a dead-end anyway.
							dep.name.clone()
						},
						Some(dep) => dep.pkg.id.to_string(), // TODO rename
					};
					dag.add_edge(
						CrateAndFeature(pkg.id.to_string(), feature.clone()),
						CrateAndFeature(dep_id.clone(), dep_feature.into()),
					);

				//log::info!(
				//	"Adding: ({}, {}) -> ({}, {})",
				//	pkg.name,
				//	feature,
				//	dep.name,
				//	dep_feature
				//);
				} else {
					let dep_feature = dep;
					// Sanity check
					assert!(pkg.features.contains_key(dep_feature));
					// Enables one of its own features.
					//log::info!(
					//	"Adding: ({}, {}) -> ({}, {})",
					//	pkg.name,
					//	feature,
					//	pkg.name,
					//	dep_feature
					//);
					dag.add_edge(
						CrateAndFeature(pkg.id.to_string(), feature.clone()),
						CrateAndFeature(pkg.id.to_string(), dep_feature.into()),
					)
				}
			}
		}
	}
	dag
}
