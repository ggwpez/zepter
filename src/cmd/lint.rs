// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Lint your feature usage by analyzing crate metadata.

mod duplicate;
mod graph;
pub mod nostd;
mod propagation;
mod relationships;

pub use duplicate::DuplicateDepsCmd;
pub(crate) use graph::resolver_one;
pub use graph::{
	build_feature_dag, build_feature_dag_with_resolver, CrateAndFeature, FeatureGraph,
};
pub use nostd::*;
pub use propagation::{DepKind, IgnoreSetting, MuteSetting, PropagateFeatureCmd};
pub use relationships::{NeverEnablesCmd, NeverImpliesCmd, OnlyEnablesCmd, WhyEnabledCmd};

use super::{CommandError, GlobalArgs};
use crate::grammar::plural;
use cargo_metadata::Package;
use std::collections::BTreeSet;

#[cfg(feature = "logging")]
const LOG_TARGET: &str = module_path!();

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

impl LintCmd {
	pub(crate) fn run(&self, global: &GlobalArgs) -> Result<(), CommandError> {
		match &self.subcommand {
			SubCommand::PropagateFeature(cmd) => cmd.run(global)?,
			SubCommand::NeverEnables(cmd) => cmd.run(global)?,
			SubCommand::NeverImplies(cmd) => cmd.run(global)?,
			SubCommand::WhyEnabled(cmd) => cmd.run(global)?,
			SubCommand::OnlyEnables(cmd) => cmd.run(global)?,
			SubCommand::NoStd(cmd) => cmd.run(global)?,
			SubCommand::DuplicateDeps(cmd) => cmd.run(global)?,
		}
		Ok(())
	}
}

fn feature_activates_dependency(pkg: &Package, feature: &str, name: &str) -> bool {
	let mut next = pkg.features.get(feature);
	let mut pending = Vec::new();
	let mut visited = BTreeSet::new();
	while let Some(entries) = next {
		for entry in entries {
			// `name?/feature` only forwards a feature if the dependency is already active.
			let dependency =
				entry.strip_prefix("dep:").or_else(|| entry.split_once('/').map(|(dep, _)| dep));
			match dependency {
				Some(dep) if dep == name => return true,
				None if visited.insert(entry) => pending.extend(pkg.features.get(entry)),
				_ => {},
			}
		}
		next = pending.pop();
	}
	false
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
		let fixed = format!(" fixed {fixes}");
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
	Some(format!("{ret}."))
}
