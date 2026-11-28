// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod collect;
mod report;

use super::{build_feature_dag, CrateAndFeature, FeatureGraph};
use crate::{
	autofix::AutoFixerArgs,
	cmd::{parse_key_val, CargoArgs, CommandError, GlobalArgs, RenamedPackage},
	CrateId,
};
use cargo_metadata::Metadata;
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	fs::canonicalize,
	path::PathBuf,
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

	/// Require FEATURE to activate the optional dependency CRATE.
	///
	/// When forwarding a missing feature, --fix adds `crate/feature` instead of `crate?/feature`.
	/// If the dependency has no such feature, --fix activates it directly, using its implicit
	/// feature when available or `dep:crate` when hidden. Use the dependency's manifest alias.
	/// Repeat the option or separate FEATURE:CRATE pairs with commas.
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

#[derive(Default)]
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
			self.run_feature(&meta, &dag, feature, global)?;
		}

		Ok(())
	}

	fn run_feature(
		&self,
		meta: &Metadata,
		dag: &FeatureGraph,
		feature: &str,
		global: &GlobalArgs,
	) -> Result<(), CommandError> {
		// Allowed dir that we can write to.
		let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).map_err(|e| {
			format!("Could not canonicalize workspace root {}: {e}", meta.workspace_root)
		})?;

		let to_check: Vec<_> = meta
			.packages
			.iter()
			.filter(|pkg| self.packages.is_empty() || self.packages.contains(&pkg.name))
			.collect();
		if to_check.is_empty() {
			return Err(format!("No packages found: {:?}", self.packages).into())
		}

		let findings = self.collect_findings(meta, dag, feature, &to_check)?;
		report::Reporter::new(self, meta, &allowed_dir, feature, findings).report(global)
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
