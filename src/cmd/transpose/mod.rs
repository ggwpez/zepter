// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod lift_to_workspace;

use super::{CommandError, GlobalArgs};
use crate::{
	autofix::*,
	cmd::{resolve_dep, transpose::lift_to_workspace::LiftToWorkspaceCmd},
};

use cargo_metadata::DependencyKind;
use std::{collections::BTreeMap as Map, fs::canonicalize};

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum SourceLocationSelector {
	/// The dependency is referenced via a `path`.
	Local,
	/// Either git or a registry.
	Remote,
}

/// Transpose dependencies in the workspace.
#[derive(Debug, clap::Parser)]
pub struct TransposeCmd {
	#[clap(subcommand)]
	subcommand: TransposeSubCmd,
}

impl TransposeCmd {
	pub fn run(&self, global: &GlobalArgs) -> Result<(), CommandError> {
		match &self.subcommand {
			TransposeSubCmd::Dependency(cmd) => cmd.run(global).map_err(Into::into),
			TransposeSubCmd::Features(cmd) => cmd.run(global),
		}
	}
}

/// Sub-commands of the [Transpose](TransposeCmd) command.
#[derive(Debug, clap::Subcommand)]
pub enum TransposeSubCmd {
	#[clap(alias = "dep", alias = "d")]
	Dependency(DependencyCmd),
	#[clap(alias = "f")]
	Features(FeaturesCmd),
}

#[derive(Debug, clap::Parser)]
pub struct DependencyCmd {
	#[clap(subcommand)]
	subcommand: DependencySubCmd,
}

impl DependencyCmd {
	pub fn run(&self, global: &GlobalArgs) -> Result<(), String> {
		match &self.subcommand {
			DependencySubCmd::LiftToWorkspace(cmd) => cmd.run(global),
		}
	}
}

#[derive(Debug, clap::Parser)]
pub struct FeaturesCmd {
	#[clap(subcommand)]
	subcommand: FeaturesSubCmd,
}

impl FeaturesCmd {
	pub fn run(&self, global: &GlobalArgs) -> Result<(), CommandError> {
		match &self.subcommand {
			FeaturesSubCmd::StripDevOnly(cmd) => cmd.run(global),
		}
	}
}

#[derive(Debug, clap::Subcommand)]
pub enum DependencySubCmd {
	#[clap(alias = "lift", alias = "l")]
	LiftToWorkspace(LiftToWorkspaceCmd),
}

#[derive(Debug, clap::Subcommand)]
pub enum FeaturesSubCmd {
	/// Strip out dev dependencies.
	StripDevOnly(StripDevDepsCmd),
}

#[derive(Debug, clap::Parser)]
pub struct StripDevDepsCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	/// Only consider these packages.
	#[clap(long, short = 'p', value_delimiter = ',', verbatim_doc_comment)]
	packages: Option<Vec<String>>,
}

impl StripDevDepsCmd {
	pub fn run(&self, g: &GlobalArgs) -> Result<(), CommandError> {
		g.warn_unstable();
		let meta = self.cargo_args.load_metadata()?;

		// Allowed dir that we can write to.
		let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).map_err(|e| {
			format!("Could not canonicalize workspace root '{}': {e}", meta.workspace_root)
		})?;

		for name in self.packages.iter().flatten() {
			if !meta.packages.iter().any(|p| p.name == name) {
				eprintln!("Could not find package named '{}'", g.red(name));
				return Err(CommandError::reported(1))
			}
		}

		let mut fixers = Map::new();
		for pkg in meta.packages.iter() {
			if let Some(fixer) = self.strip_package(g, &meta, pkg, &allowed_dir)? {
				fixers.insert(pkg.name.clone(), fixer);
			}
		}

		for fixer in fixers.values_mut() {
			fixer.save()?;
		}

		Ok(())
	}

	fn strip_package(
		&self,
		g: &GlobalArgs,
		meta: &cargo_metadata::Metadata,
		pkg: &cargo_metadata::Package,
		allowed_dir: &std::path::Path,
	) -> Result<Option<AutoFixer>, CommandError> {
		if self.packages.as_ref().is_some_and(|packages| !packages.contains(&pkg.name)) {
			return Ok(None)
		}

		let krate_path = canonicalize(pkg.manifest_path.as_std_path())
			.map_err(|e| format!("Could not canonicalize manifest '{}': {e}", pkg.manifest_path))?;
		if !krate_path.starts_with(allowed_dir) {
			return Ok(None)
		}
		let mut fixer = AutoFixer::from_manifest(&krate_path)?;

		let kind = DependencyKind::Development;
		let devs = pkg.dependencies.iter().filter(|d| d.kind == kind);
		let only_dev = devs
			.filter(|dev| {
				pkg.dependencies.iter().filter(|d| d.name == dev.name).all(|d| d.kind == kind)
			})
			.collect::<Vec<_>>();

		for dep in only_dev.iter() {
			let Some(dep) = resolve_dep(pkg, dep, meta) else {
				eprintln!("Could not resolve dependency '{}'", g.red(&dep.name));
				return Err(CommandError::reported(1))
			};
			fixer.remove_feature(&format!("{}/", dep.name()))?;
			fixer.remove_feature(&format!("{}?/", dep.name()))?;
		}

		Ok(fixer.modified().then_some(fixer))
	}
}
