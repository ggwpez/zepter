// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod lift_to_workspace;

use super::GlobalArgs;
use crate::{
	autofix::*,
	cmd::{resolve_dep, transpose::lift_to_workspace::LiftToWorkspaceCmd},
};

use cargo_metadata::{Dependency as Dep, DependencyKind};
use semver::{Op, Version, VersionReq};
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
	pub fn run(&self, global: &GlobalArgs) -> Result<(), String> {
		match &self.subcommand {
			TransposeSubCmd::Dependency(cmd) => cmd.run(global),
			TransposeSubCmd::Features(cmd) => {
				cmd.run(global);
				Ok(())
			},
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
	pub fn run(&self, global: &GlobalArgs) {
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
	pub fn run(&self, g: &GlobalArgs) {
		g.warn_unstable();
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");

		let kind = DependencyKind::Development;
		// Allowed dir that we can write to.
		let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).unwrap();

		for name in self.packages.iter().flatten() {
			if !meta.packages.iter().any(|p| p.name == *name) {
				eprintln!("Could not find package named '{}'", g.red(name));
				std::process::exit(1);
			}
		}

		let mut fixers = Map::new();
		for pkg in meta.packages.iter() {
			if let Some(packages) = &self.packages {
				if !packages.contains(&pkg.name) {
					continue
				}
			}

			// Are we allowed to modify this file path?
			let krate_path = canonicalize(pkg.manifest_path.clone().into_std_path_buf()).unwrap();
			if !krate_path.starts_with(&allowed_dir) {
				continue
			}
			let mut fixer = AutoFixer::from_manifest(&krate_path).unwrap();

			// Find all dependencies that are only used as dev dependencies in this package.
			let devs = pkg.dependencies.iter().filter(|d| d.kind == kind);
			let only_dev = devs
				.filter(|dev| {
					pkg.dependencies.iter().filter(|d| d.name == dev.name).all(|d| d.kind == kind)
				})
				.collect::<Vec<_>>();

			for dep in only_dev.iter() {
				// Account for renamed crates:
				let Some(dep) = resolve_dep(pkg, dep, &meta) else {
					eprintln!("Could not resolve dependency '{}'", g.red(&dep.name));
					std::process::exit(1);
				};

				fixer.remove_feature(&format!("{}/", dep.name()));
				fixer.remove_feature(&format!("{}?/", dep.name()));
			}

			if fixer.modified() {
				fixers.insert(pkg.name.clone(), fixer);
			}
		}

		for fixer in fixers.values_mut() {
			fixer.save().unwrap();
		}
	}
}
