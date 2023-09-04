// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{autofix::*, grammar::*, log};
use cargo_metadata::DependencyKind as DepKind;
use std::{
	collections::{BTreeMap as Map, HashMap},
	fs::canonicalize,
};

use super::GlobalArgs;

/// Transpose dependencies in the workspace.
#[derive(Debug, clap::Parser)]
pub struct TransposeCmd {
	#[clap(subcommand)]
	subcommand: TransposeSubCmd,
}

impl TransposeCmd {
	pub fn run(&self, global: &GlobalArgs) {
		match &self.subcommand {
			TransposeSubCmd::Dependency(cmd) => cmd.run(global),
		}
	}
}

/// Sub-commands of the [Transpose](TransposeCmd) command.
#[derive(Debug, clap::Subcommand)]
pub enum TransposeSubCmd {
	#[clap(alias = "dep", alias = "d")]
	Dependency(DependencyCmd),
}

#[derive(Debug, clap::Parser)]
pub struct DependencyCmd {
	#[clap(subcommand)]
	subcommand: DependencySubCmd,
}

impl DependencyCmd {
	pub fn run(&self, global: &GlobalArgs) {
		match &self.subcommand {
			DependencySubCmd::LiftToWorkspace(cmd) => cmd.run(global),
		}
	}
}

#[derive(Debug, clap::Subcommand)]
pub enum DependencySubCmd {
	#[clap(alias = "lift", alias = "l")]
	LiftToWorkspace(LiftToWorkspaceCmd),
}

/// Lift up a dependency to the workspace and reference it from all packages.
#[derive(Debug, clap::Parser)]
pub struct LiftToWorkspaceCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	#[clap(index(1))]
	dependency: String,

	#[clap(long, value_enum, default_value_t = DefaultFeatureMode::False)]
	default_feature: DefaultFeatureMode,
}

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum DefaultFeatureMode {
	False,
}

impl LiftToWorkspaceCmd {
	pub fn run(&self, global: &GlobalArgs) {
		let mut args = self.cargo_args.clone();
		args.workspace = true;
		let meta = args.load_metadata().expect("Loads metadata");
		log::debug!("Scanning workspace for '{}'", self.dependency);
		// crate -> dependency
		let mut found = Vec::new();
		let mut by_kind = HashMap::<DepKind, u32>::new();
		let mut found_version: Option<cargo_metadata::semver::VersionReq> = None;

		for pkg in meta.packages.iter() {
			for dep in pkg.dependencies.iter() {
				if dep.name != self.dependency {
					continue
				}

				found.push((pkg.clone(), dep.clone()));

				if found_version.as_ref().map_or(false, |f| f.ne(&dep.req)) {
					panic!(
						"Found different versions of '{}' in the workspace: {} vs {}. Please use 'cargo upgrade -p {}' first.",
						global.bold(&self.dependency), global.red(&format!("{}", found_version.unwrap())), global.red(&format!("{}", dep.req)), &self.dependency
					);
				}
				found_version = Some(dep.req.clone());
				log::debug!(
					"Found '{}' in package '{}' with version '{}'",
					self.dependency,
					pkg.name,
					dep.req
				);
				*by_kind.entry(dep.kind).or_default() += 1;
			}
		}
		let Some(version) = found_version else {
			panic!("Could not find any dependency named '{}'", global.red(&self.dependency));
		};

		log::info!(
			"Selected '{} {}' for lift up ({} occurrence{}: N={}, D={}, B={})",
			&self.dependency,
			&version,
			found.len(),
			plural(found.len()),
			by_kind.get(&DepKind::Normal).unwrap_or(&0),
			by_kind.get(&DepKind::Development).unwrap_or(&0),
			by_kind.get(&DepKind::Build).unwrap_or(&0)
		);

		let mut fixers = Map::new();
		for (pkg, dep) in found {
			let krate_path = canonicalize(pkg.manifest_path.clone().into_std_path_buf()).unwrap();
			fixers
				.entry(pkg.name.clone())
				.or_insert_with(|| AutoFixer::from_manifest(&krate_path).unwrap());
			let fixer = fixers.get_mut(&pkg.name).unwrap();

			fixer.lift_dependency(&dep.name, dep.uses_default_features).unwrap(); // TODO
		}

		for fixer in fixers.values_mut() {
			fixer.save().unwrap();
		}

		// Now create fixer for the root package
		//let mut fixer =
		// AutoFixer::from_manifest(&meta.workspace_root.into_std_path_buf()).unwrap();
		// fixer.add_workspace_dep(&found_dep.unwrap(), false);
		//fixer.save().unwrap();
	}
}
