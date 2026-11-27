// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{build_feature_dag, error_stats, CrateAndFeature};
use crate::{
	autofix::{AutoFixer, AutoFixerArgs},
	cmd::{parse_key_val, resolve_dep, CargoArgs, CommandError, GlobalArgs, RenamedPackage},
	dag::Dag,
	log, CrateId,
};
use cargo_metadata::{Metadata, Package};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	fs::canonicalize,
	path::{Path, PathBuf},
};

/// Verifies that rust features are properly propagated.
#[derive(Debug, clap::Parser)]
pub struct PropagateFeatureCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,

	/// Comma separated list of features to check.
	///
	/// Listing the same feature multiple times has the same effect as listing it once.
	#[clap(long, alias = "feature", value_delimiter = ',', required = true)]
	features: Vec<String>,

	/// The packages to check. If empty, all packages are checked.
	#[clap(long, short, num_args(0..))]
	packages: Vec<String>,

	/// Enables the feature of the dependencies as non-optional.
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
			_ => Err(format!("Unknown dependency kind '{s}'")),
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
			_ => Err(format!("Unknown ignore setting '{s}'")),
		}
	}
}

type MissingDependencies = BTreeMap<CrateId, BTreeSet<RenamedPackage>>;

struct Findings {
	propagate_missing: MissingDependencies,
	feature_missing: MissingDependencies,
	non_optional_missing: MissingDependencies,
}

impl PropagateFeatureCmd {
	pub fn run(&self, global: &GlobalArgs) -> Result<(), CommandError> {
		let meta = self.cargo_args.load_metadata()?;
		let dag = build_feature_dag(&meta, &meta.packages)?;
		let features = self.features.iter().collect::<BTreeSet<_>>();

		for feature in features {
			self.run_feature(&meta, &dag, feature.clone(), global)?;
		}

		Ok(())
	}

	fn run_feature(
		&self,
		meta: &Metadata,
		dag: &Dag<CrateAndFeature>,
		feature: String,
		global: &GlobalArgs,
	) -> Result<(), CommandError> {
		// Allowed dir that we can write to.
		let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).map_err(|e| {
			format!("Could not canonicalize workspace root {}: {e}", meta.workspace_root)
		})?;

		let pkgs = meta.packages.iter().collect::<Vec<_>>();
		let mut to_check = pkgs.clone();
		if !self.packages.is_empty() {
			to_check =
				pkgs.iter().filter(|pkg| self.packages.contains(&pkg.name)).cloned().collect();
		}
		if to_check.is_empty() {
			return Err(format!("No packages found: {:?}", self.packages).into())
		}

		let findings = self.collect_findings(meta, dag, &feature, &to_check)?;
		self.report_findings(meta, &pkgs, &allowed_dir, &feature, global, findings)
	}

	fn collect_findings(
		&self,
		meta: &Metadata,
		dag: &Dag<CrateAndFeature>,
		feature: &String,
		to_check: &[&Package],
	) -> Result<Findings, String> {
		// (Crate that is not forwarding the feature) -> (Dependency that it is not forwarded to)
		let mut propagate_missing = BTreeMap::<CrateId, BTreeSet<RenamedPackage>>::new();
		let ignore_missing_propagate = self.ignore_missing_propagate()?;
		let dep_kinds = self.parse_dep_kinds()?;
		// (Crate that missing the feature) -> (Dependency that has it)
		let mut feature_missing = BTreeMap::<CrateId, BTreeSet<RenamedPackage>>::new();
		// (Crate that is not enabling a optional dependency) -> (Dependency that it is not enabled)
		let mut non_optional_missing = BTreeMap::<CrateId, BTreeSet<RenamedPackage>>::new();

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

				// If optional and require by `feature-enables-dep` but does not have the feature,
				// then we need to enable it as non-optional.
				if dep.optional {
					if let Some((feature, _)) = self
						.feature_enables_dep
						.iter()
						.flatten()
						.find(|(f, name)| f == feature && *name == dep.name())
					{
						if !dep.pkg.features.contains_key(feature) &&
							!pkg.features.get(feature).is_some_and(|f| f.contains(&dep.name()))
						{
							non_optional_missing
								.entry(pkg.id.to_string())
								.or_default()
								.insert(dep.clone());
						}
					}
					// Continue here should not make a difference. TODO check.
				}

				if !dep.pkg.features.contains_key(feature) {
					continue
				}

				if !pkg.features.contains_key(feature) {
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
						target: super::LOG_TARGET,
						"Reachable from the default entrypoint: {:?} vis {:?}",
						target,
						p.0
					);
					continue
				}

				if let Some((_, lhs_ignore)) = ignore_missing_propagate
					.iter()
					.find(|(c, _)| pkg.name == c.0 && &c.1 == feature)
				{
					if lhs_ignore.iter().any(|i| dep.pkg.name == i.0 && &i.1 == feature) {
						continue
					}
				}

				propagate_missing.entry(pkg.id.to_string()).or_default().insert(dep);
			}
		}
		Ok(Findings { propagate_missing, feature_missing, non_optional_missing })
	}

	fn report_findings(
		&self,
		meta: &Metadata,
		pkgs: &[&Package],
		allowed_dir: &Path,
		feature: &String,
		global: &GlobalArgs,
		findings: Findings,
	) -> Result<(), CommandError> {
		let Findings { propagate_missing, feature_missing, non_optional_missing } = findings;
		let lookup = |id: &str| {
			pkgs.iter()
				.find(|pkg| pkg.id.repr == id)
				.copied()
				.ok_or_else(|| format!("Could not find crate {id} in the metadata"))
		};

		let faulty_crates: BTreeSet<CrateId> = propagate_missing
			.keys()
			.chain(feature_missing.keys())
			.chain(non_optional_missing.keys())
			.cloned()
			.collect();
		let mut faulty_crates = faulty_crates
			.into_iter()
			.map(|id| lookup(&id).map(|pkg| (pkg, id)))
			.collect::<Result<Vec<_>, _>>()?;
		faulty_crates.sort_by(|(a, _), (b, _)| a.name.cmp(&b.name));

		let (mut errors, mut fixes) = (0, 0);
		for (krate, _) in faulty_crates {
			let in_workspace = meta.workspace_members.iter().any(|m| m == &krate.id);
			// check if we can modify in allowed_dir
			let krate_path = canonicalize(krate.manifest_path.as_std_path()).map_err(|e| {
				format!("Could not canonicalize manifest {}: {e}", krate.manifest_path)
			})?;
			// TODO move down
			let mut fixer = if self.fixer_args.enable {
				if krate_path.starts_with(allowed_dir) ||
					self.modify_paths.iter().any(|p| krate_path.starts_with(p))
				{
					Some(AutoFixer::from_manifest(&krate_path)?)
				} else {
					log::info!(
						target: super::LOG_TARGET,
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

			println!("crate {krate_str}\n  feature '{feature}'",);

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
					self.fix_package.as_ref().is_none_or(|p| p == &krate.name.to_string()) &&
					self.left_side_feature_missing == MuteSetting::Fix &&
					(self.left_side_outside_workspace == MuteSetting::Fix || in_workspace)
				{
					let Some(fixer) = fixer.as_mut() else { continue };
					fixer.add_feature(feature)?;

					log::info!(
						target: super::LOG_TARGET,
						"Inserted feature '{}' into '{}'",
						&feature,
						&krate.name
					);
					fixes += 1;
				}

				errors += 1;
			}

			if let Some(deps) = propagate_missing.get(&krate.id.to_string()) {
				let mut named = deps.iter().map(RenamedPackage::display_name).collect::<Vec<_>>();
				named.sort();
				println!("    must propagate to:\n      {}", named.join("\n      "));

				if self.fixer_args.enable &&
					self.fix_package.as_ref().is_none_or(|p| p == &krate.name.to_string())
				{
					for dep in deps.iter() {
						let dep_name = dep.name();
						if self.fix_dependency.as_ref().is_some_and(|d| d != &dep_name) {
							continue
						}
						let Some(fixer) = fixer.as_mut() else { continue };
						let non_optional = self
							.feature_enables_dep
							.as_ref()
							.is_some_and(|v| v.contains(&(feature.clone(), dep_name.clone())));
						let opt = if !non_optional && dep.optional { "?" } else { "" };

						fixer.add_to_feature(
							feature,
							format!("{dep_name}{opt}/{feature}").as_str(),
						)?;
						log::info!(
							target: super::LOG_TARGET,
							"Inserted '{dep_name}/{feature}' into '{}'",
							krate.name
						);
						fixes += 1;
					}
				}
				errors += deps.len();
			}

			if let Some(deps) = non_optional_missing.get(&krate.id.to_string()) {
				let mut named = deps.iter().map(RenamedPackage::display_name).collect::<Vec<_>>();
				named.sort();
				println!(
					"    must enable dependency as non-optional:\n      {}",
					named.join("\n      ")
				);

				if self.fixer_args.enable &&
					self.fix_package.as_ref().is_none_or(|p| p == &krate.name.to_string())
				{
					for dep in deps.iter() {
						let dep_name = dep.name();
						let Some(fixer) = fixer.as_mut() else { continue };

						fixer.add_to_feature(feature, &dep_name)?;

						log::info!(
							target: super::LOG_TARGET,
							"Inserted '{dep_name}' into '{}'",
							krate.name
						);
						fixes += 1;
					}
				}

				errors += deps.len();
			}

			if let Some(fixer) = fixer.as_mut() {
				if fixes > 0 {
					fixer.save()?;
				}
			}
		}
		if let Some(e) = error_stats(errors, 0, fixes, self.fixer_args.enable, global) {
			println!("{e}");
		}

		if errors > fixes {
			return Err(CommandError::reported(global.error_code()))
		}
		Ok(())
	}

	fn ignore_missing_propagate(
		&self,
	) -> Result<BTreeMap<CrateAndFeature, BTreeSet<CrateAndFeature>>, String> {
		let Some(ignore_missing) = &self.ignore_missing_propagate else {
			return Ok(Default::default())
		};

		let mut map = BTreeMap::<CrateAndFeature, BTreeSet<CrateAndFeature>>::new();
		for (lhs, rhs) in ignore_missing {
			let (lhs_c, lhs_f) = lhs
				.split_once('/')
				.ok_or_else(|| format!("Expected CRATE/FEATURE, got '{lhs}'"))?;
			let (rhs_c, rhs_f) = rhs
				.split_once('/')
				.ok_or_else(|| format!("Expected CRATE/FEATURE, got '{rhs}'"))?;

			let lhs = CrateAndFeature(lhs_c.into(), lhs_f.into());
			let rhs = CrateAndFeature(rhs_c.into(), rhs_f.into());

			map.entry(lhs).or_default().insert(rhs);
		}
		Ok(map)
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
