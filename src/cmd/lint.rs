// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Lint your feature usage by analyzing crate metadata.

pub mod nostd;
pub use nostd::*;

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
	collections::{BTreeMap, BTreeSet, HashMap, HashSet},
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
	/// Check the crates for sane no-std feature configuration.
	NoStd(NoStdCmd),
	/// Check for duplicated dependencies in `[dependencies]` and `[dev-dependencies]`.
	DuplicateDeps(DuplicateDepsCmd),
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

	/// Comma separated list of features to check.
	///
	/// Listing the same feature multiple times has the same effect as listing it once.
	#[clap(long, alias = "feature", value_delimiter = ',', required = true)]
	features: Vec<String>,

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
	#[clap(long, value_enum, value_name = "MUTE_SETTING", default_value_t = MuteSetting::Fix, verbatim_doc_comment)]
	left_side_feature_missing: MuteSetting,

	/// Ignore single missing links in the feature propagation chain.
	#[clap(long, value_name = "CRATE/FEATURE:DEP/DEP_FEATURE", value_parser = parse_key_val::<String, String>, value_delimiter = ',', verbatim_doc_comment)]
	ignore_missing_propagate: Option<Vec<(String, String)>>,

	/// How to handle the case that the LHS is outside the workspace.
	#[clap(long, value_enum, value_name = "MUTE_SETTING", default_value_t = MuteSetting::Fix, verbatim_doc_comment)]
	left_side_outside_workspace: MuteSetting,

	/// How to handle dev-dependencies.
	#[clap(long, value_name = "KIND/MUTE_SETTING", value_parser = parse_key_val::<String, String>, value_delimiter = ',', verbatim_doc_comment, default_value = "normal:check,dev:check,build:check")]
	dep_kinds: Option<Vec<(String, String)>>,

	/// Show crate versions in the output.
	#[clap(long)]
	show_version: bool,

	/// Show crate manifest paths in the output.
	#[clap(long)]
	show_path: bool,

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
	/// Only report but do not fix.
	Report,
	/// Fix if `--fix` is passed.
	Fix,
}

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum IgnoreSetting {
	Ignore,
	Check,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Copy)]
pub enum DepKind {
	Normal,
	Dev,
	Build,
}

impl core::str::FromStr for DepKind {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_ascii_lowercase().as_str() {
			"normal" => Ok(Self::Normal),
			"dev" => Ok(Self::Dev),
			"build" => Ok(Self::Build),
			_ => Err(format!("Unknown dependency kind '{}'", s)),
		}
	}
}

impl From<DepKind> for cargo_metadata::DependencyKind {
	// oh god, someone clean this up.
	fn from(kind: DepKind) -> Self {
		match kind {
			DepKind::Normal => Self::Normal,
			DepKind::Dev => Self::Development,
			DepKind::Build => Self::Build,
		}
	}
}

impl core::str::FromStr for IgnoreSetting {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_ascii_lowercase().as_str() {
			"ignore" => Ok(Self::Ignore),
			"check" => Ok(Self::Check),
			_ => Err(format!("Unknown ignore setting '{}'", s)),
		}
	}
}

impl LintCmd {
	pub(crate) fn run(&self, global: &GlobalArgs) -> Result<(), String> {
		match &self.subcommand {
			SubCommand::PropagateFeature(cmd) => cmd.run(global),
			SubCommand::NeverEnables(cmd) => {
				cmd.run(global);
				Ok(())
			},
			SubCommand::NeverImplies(cmd) => {
				cmd.run(global);
				Ok(())
			},
			SubCommand::WhyEnabled(cmd) => {
				cmd.run(global);
				Ok(())
			},
			SubCommand::OnlyEnables(cmd) => {
				cmd.run(global);
				Ok(())
			},
			SubCommand::NoStd(cmd) => cmd.run(global),
			SubCommand::DuplicateDeps(cmd) => {
				cmd.run(global);
				Ok(())
			},
		}
	}
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd, Debug, Hash)]
pub struct CrateAndFeature(pub String, pub String);

impl Display for CrateAndFeature {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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
					let referenced_crate =
						pkgs.iter().find(|pkg| pkg.id.to_string() == id).unwrap();
					pkgs.iter()
						.find(|pkg| pkg.id == referenced_crate.id)
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
			if enabled.contains(&self.stays_disabled) {
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
			// Find by id
			let lhs = pkgs.iter().find(|p| p.id.to_string() == lhs).unwrap();
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
	}
}

impl PropagateFeatureCmd {
	pub fn run(&self, global: &GlobalArgs) -> Result<(), String> {
		let meta = self.cargo_args.load_metadata()?;
		let dag = build_feature_dag(&meta, &meta.packages);
		let features = self.features.iter().collect::<BTreeSet<_>>();

		for feature in features.into_iter() {
			self.run_feature(&meta, &dag, feature.clone(), global);
		}

		Ok(())
	}

	fn run_feature(
		&self,
		meta: &Metadata,
		dag: &Dag<CrateAndFeature>,
		feature: String,
		global: &GlobalArgs,
	) {
		// Allowed dir that we can write to.
		let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).unwrap();

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
		let ignore_missing_propagate = self.ignore_missing_propagate();
		let dep_kinds = self.parse_dep_kinds().expect("Parse dependency kinds");
		// (Crate that missing the feature) -> (Dependency that has it)
		let mut feature_missing = BTreeMap::<CrateId, BTreeSet<RenamedPackage>>::new();

		for pkg in to_check.iter() {
			// TODO that it does not enable other features.
			let in_workspace = meta.workspace_members.iter().any(|m| m == &pkg.id);
			if !in_workspace && self.left_side_outside_workspace == MuteSetting::Ignore {
				continue
			}

			for dep in pkg.dependencies.iter() {
				let mute = dep_kinds.get(&dep.kind).unwrap_or(&IgnoreSetting::Check);
				if mute == &IgnoreSetting::Ignore {
					continue
				}
				// TODO handle default features.
				// Resolve the dep according to the metadata.
				let Some(dep) = resolve_dep(pkg, dep, meta) else {
					// Either outside workspace or not resolved, possibly due to not being used at
					// all because of the target or whatever.
					continue
				};

				if !dep.pkg.features.contains_key(&feature) {
					continue
				}
				if !pkg.features.contains_key(&feature) {
					if self.left_side_feature_missing != MuteSetting::Ignore {
						feature_missing.entry(pkg.id.to_string()).or_default().insert(dep);
					}
					continue
				}

				// TODO check that optional deps are only enabled as optional unless
				// overwritten with `--feature-enables-dep`.
				let target = CrateAndFeature(dep.pkg.id.repr.clone(), feature.clone());
				let want_opt = CrateAndFeature(format!("{}?", &pkg.id), feature.clone());
				let want_req = CrateAndFeature(pkg.id.repr.clone(), feature.clone());

				if dag.adjacent(&want_opt, &target) || dag.adjacent(&want_req, &target) {
					// Easy case, all good.
					continue
				}
				let default_entrypoint = CrateAndFeature(pkg.id.repr.clone(), "#entrypoint".into());
				// Now the more complicated case where `pkg/F -> dep/G .. -> dep/F`. So to say a
				// multi-hop internal transitive propagation of the feature on the dependency side.
				let sub_dag = dag.sub(|CrateAndFeature(p, f)| {
					(p == &pkg.id.repr && f == "#entrypoint") || (p == &dep.pkg.id.repr)
				});
				if let Some(p) = sub_dag.any_path(&default_entrypoint, &target) {
					let _ = p;
					// Easy case, all good.
					log::debug!(
						"Reachable from the default entrypoint: {:?} vis {:?}",
						target,
						p.0
					);
					continue
				}

				if let Some((_, lhs_ignore)) =
					ignore_missing_propagate.iter().find(|(c, _)| pkg.name == c.0 && c.1 == feature)
				{
					if lhs_ignore.iter().any(|i| dep.pkg.name == i.0 && i.1 == feature) {
						continue
					}
				}

				propagate_missing.entry(pkg.id.to_string()).or_default().insert(dep);
			}
		}
		let faulty_crates: BTreeSet<CrateId> =
			propagate_missing.keys().chain(feature_missing.keys()).cloned().collect();
		let mut faulty_crates =
			faulty_crates.into_iter().map(|id| (lookup(&id), id)).collect::<Vec<_>>();
		faulty_crates.sort_by(|(a, _), (b, _)| a.name.cmp(&b.name));

		let (mut errors, mut fixes) = (0, 0);
		for (krate, _) in faulty_crates {
			let in_workspace = meta.workspace_members.iter().any(|m| m == &krate.id);
			// check if we can modify in allowed_dir
			let krate_path = canonicalize(krate.manifest_path.clone().into_std_path_buf()).unwrap();
			// TODO move down
			let mut fixer = if self.fixer_args.enable {
				if krate_path.starts_with(&allowed_dir) ||
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

			let mut krate_str = format!("'{}'", krate.name);
			if self.show_path {
				krate_str.push_str(&format!(" ({})", krate.manifest_path));
			}

			println!("crate {krate_str}\n  feature '{}'", feature);

			if let Some(deps) = feature_missing.get(&krate.id.to_string()) {
				let mut named = deps.iter().map(RenamedPackage::display_name).collect::<Vec<_>>();
				named.sort();
				println!(
					"    is required by {} dependenc{}:\n      {}",
					deps.len(),
					if deps.len() == 1 { "y" } else { "ies" },
					named.join("\n      "),
				);

				if self.fixer_args.enable &&
					self.fix_package.as_ref().map_or(true, |p| p == &krate.name) &&
					self.left_side_feature_missing == MuteSetting::Fix &&
					(self.left_side_outside_workspace == MuteSetting::Fix || in_workspace)
				{
					let Some(fixer) = fixer.as_mut() else { continue };
					fixer.add_feature(&feature).unwrap();

					log::info!("Inserted feature '{}' into '{}'", &feature, &krate.name);
					fixes += 1;
				}

				errors += 1;
			}

			if let Some(deps) = propagate_missing.get(&krate.id.to_string()) {
				let mut named = deps.iter().map(RenamedPackage::display_name).collect::<Vec<_>>();
				named.sort();
				println!("    must propagate to:\n      {}", named.join("\n      "));

				if self.fixer_args.enable &&
					self.fix_package.as_ref().map_or(true, |p| p == &krate.name)
				{
					for dep in deps.iter() {
						let dep_name = dep.name();
						if !self.fix_dependency.as_ref().map_or(true, |d| d == &dep_name) {
							continue
						}
						let Some(fixer) = fixer.as_mut() else { continue };
						let non_optional = self
							.feature_enables_dep
							.as_ref()
							.is_some_and(|v| v.contains(&(feature.clone(), dep_name.clone())));
						let opt = if !non_optional && dep.optional { "?" } else { "" };

						fixer
							.add_to_feature(
								&feature,
								format!("{}{}/{}", dep_name, opt, feature).as_str(),
							)
							.unwrap();
						log::info!("Inserted '{dep_name}/{feature}' into '{}'", krate.name);
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
		}
		if let Some(e) = error_stats(errors, 0, fixes, self.fixer_args.enable, global) {
			println!("{}", e);
		}

		if errors > fixes {
			std::process::exit(global.error_code());
		}
	}

	fn ignore_missing_propagate(&self) -> BTreeMap<CrateAndFeature, BTreeSet<CrateAndFeature>> {
		let Some(ignore_missing) = &self.ignore_missing_propagate else {
			return Default::default()
		};

		let mut map = BTreeMap::<CrateAndFeature, BTreeSet<CrateAndFeature>>::new();
		for (lhs, rhs) in ignore_missing {
			let (lhs_c, lhs_f) = lhs.split_once('/').unwrap();
			let (rhs_c, rhs_f) = rhs.split_once('/').unwrap();

			let lhs = CrateAndFeature(lhs_c.into(), lhs_f.into());
			let rhs = CrateAndFeature(rhs_c.into(), rhs_f.into());

			map.entry(lhs).or_default().insert(rhs);
		}
		map
	}

	fn parse_dep_kinds(
		&self,
	) -> Result<HashMap<cargo_metadata::DependencyKind, IgnoreSetting>, String> {
		let mut map = HashMap::new();
		if let Some(kinds) = &self.dep_kinds {
			for (kind, mute) in kinds {
				map.insert(kind.parse::<DepKind>()?.into(), mute.parse()?);
			}
		}
		Ok(map)
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
			if fixes == warnings + errors {
				ret.push_str(" (all fixed)");
			}
		} else {
			ret.push_str(&fixed);
		}

		if fixes < errors {
			let could_not = format!(" ({} could not be fixed)", errors - fixes);
			ret.push_str(&global.red(&could_not));
		}
	} else if global.show_hints() {
		ret.push_str(" (run with `--fix` to fix)");
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
					}
					log::info!("{}: {}", feat, imply.join(", "));

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
		debug_assert!(!enabled_by.is_empty());
		println!("Feature {}/{} is enabled by:", self.feature, self.package);
		for (name, feature) in enabled_by {
			println!("  {}/{}", name, feature);
		}
	}
}

#[derive(Debug, clap::Parser)]
pub struct DuplicateDepsCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,
}

impl DuplicateDepsCmd {
	pub fn run(&self, _global: &GlobalArgs) {
		// To easily compare dependencies, we normalize them by removing the kind.
		fn normalize_dep(dep: &cargo_metadata::Dependency) -> cargo_metadata::Dependency {
			let mut dep = dep.clone();
			dep.kind = cargo_metadata::DependencyKind::Unknown;
			dep
		}

		let meta = self.cargo_args.load_metadata().expect("Loads metadata");

		let mut issues = vec![];

		for pkg in &meta.workspace_packages() {
			let deps: HashSet<_> = pkg
				.dependencies
				.iter()
				.filter(|d| d.kind == cargo_metadata::DependencyKind::Normal)
				.map(normalize_dep)
				.collect();

			let dev_deps: HashSet<_> = pkg
				.dependencies
				.iter()
				.filter(|d| d.kind == cargo_metadata::DependencyKind::Development)
				.map(normalize_dep)
				.collect();

			for dep in deps.intersection(&dev_deps) {
				issues.push(format!(
					"Package `{}` has duplicated `{}` in both [dependencies] and [dev-dependencies]",
					pkg.name, dep.name
				));
			}
		}

		if !issues.is_empty() {
			for issue in issues {
				println!("{issue}");
			}
			std::process::exit(1);
		}
	}
}
// Complexity is `O(x ^ 4) with x=pkgs.len()`.
pub fn build_feature_dag(meta: &Metadata, pkgs: &[Package]) -> Dag<CrateAndFeature> {
	let mut dag = Dag::new();

	for pkg in pkgs.iter() {
		for dep in &pkg.dependencies {
			if dep.uses_default_features {
				dag.add_edge(
					CrateAndFeature(pkg.id.to_string(), "default".into()),
					CrateAndFeature(dep.name.clone(), "default".into()),
				);

				let Some(dep_id) = resolve_dep(pkg, dep, meta) else { continue };

				// Hacky…
				dag.add_edge(
					CrateAndFeature(pkg.id.to_string(), "#entrypoint".into()),
					CrateAndFeature(dep_id.pkg.id.repr.clone(), "default".into()),
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
					let dep = pkg
						.dependencies
						.iter()
						.find(|d| d.rename.as_ref().unwrap_or(&d.name) == dep)
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
						.find(|d| d.rename.as_ref().unwrap_or(&d.name) == &dep)
						.unwrap_or_else(|| panic!("Could not resolve dep {} of {}", dep, pkg.id));

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
				} else {
					let dep_feature = dep;
					// Sanity check
					debug_assert!(pkg.features.contains_key(dep_feature));
					// Enables one of its own features.
					dag.add_edge(
						CrateAndFeature(pkg.id.to_string(), feature.clone()),
						CrateAndFeature(pkg.id.to_string(), dep_feature.into()),
					);
				}
			}
		}
	}
	dag
}
