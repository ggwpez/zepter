// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::GlobalArgs;
use crate::{autofix::*, grammar::*, log};

use cargo_metadata::{Dependency as Dep, Package};
use itertools::Itertools;
use semver::{Version, VersionReq, Op};
use std::{
	collections::{BTreeMap as Map, HashMap},
	fs::canonicalize,
};

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
	pub fn run(&self, g: &GlobalArgs) {
		g.warn_unstable();

		let mut args = self.cargo_args.clone();
		args.workspace = true;
		let meta = args.load_metadata().expect("Loads metadata");
		log::debug!("Scanning workspace for '{}'", self.dependency);
		// version -> crate
		let mut by_version = HashMap::<semver::VersionReq, Vec<(Package, Dep)>>::new();

		for pkg in meta.packages.iter() {
			for dep in pkg.dependencies.iter() {
				if dep.name != self.dependency {
					continue
				}

				by_version.entry(dep.req.clone()).or_default().push((pkg.clone(), dep.clone()));
			}
		}

		let versions = by_version.keys().collect::<Vec<_>>();
		if versions.len() > 1 {
			let str_width = versions.iter().map(|v| v.to_string().len()).max().unwrap();
			let mut err = String::new();
			// iter by descending frequence
			for (version, pkgs) in by_version.iter().sorted_by_key(|(_, pkgs)| pkgs.len()).rev() {
				let ddd = if pkgs.len() > 3 { ", …" } else { "" };
				let s = plural_or(pkgs.len(), " ");
				// TODO plural s
				err.push_str(&format!(
					"  {: <width$}: {: >3} time{s} ({}{ddd})\n",
					version.to_string(),
					pkgs.len(),
					pkgs.iter()
						.map(|(c, _)| c.name.as_str())
						.take(3)
						.collect::<Vec<_>>()
						.join(", "),
					width = str_width
				));
			}

			let version_hint = match try_find_latest(by_version.keys()) {
				Ok(latest) => latest.to_string(),
				Err(e) => {
					log::warn!("Could not find determine latest common version: {}", e);
					"version".to_string()
				},
			};
			let hint = format!("cargo upgrade -p {}@{version_hint}", &self.dependency);
			panic!(
				"\nFound {} different versions of '{}' in the workspace:\n\n{err}\nHint: {}\n",
				versions.len(),
				&self.dependency,
				g.bold(&hint),
			);
		}

		let Some(version) = by_version.keys().next() else {
			panic!("Could not find any dependency named '{}'", g.red(&self.dependency));
		};
		let _ = version;
		let found = by_version.values().map(Vec::len).sum();

		log::info!(
			"Selected '{} {}' for lift up ({} occurrence{})", //: N={}, D={}, B={})",
			&self.dependency,
			&version,
			found,
			crate::grammar::plural(found),
		);

		let mut fixers = Map::new();
		for (pkg, dep) in by_version.values().flatten() {
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
		let root_manifest_path = meta.workspace_root.join("Cargo.toml");
		let mut fixer =
		 AutoFixer::from_manifest(&root_manifest_path.into_std_path_buf()).unwrap();
		let dep = by_version.values().next().unwrap().first().unwrap().1.clone();
		fixer.add_workspace_dep(&dep, false).unwrap();
		fixer.save().unwrap();
	}
}

fn try_find_latest<'a, I: Iterator<Item = &'a VersionReq>>(reqs: I) -> Result<Version, String> {
	let mut versions = Vec::<Version>::new();

	// Try to convert each to a version. This is done as best-effort:
	for req in reqs {
		if req.comparators.len() != 1 {
			return Err(format!("Invalid version requirement: '{}'", req));
		}
		let comp = req.comparators.first().unwrap();
		if comp.op != Op::Caret {
			return Err(format!("Only caret is supported, but got: '{}'", req));
		}
		if !comp.pre.is_empty() {
			return Err(format!("Pre-release versions are not supported: '{}'", req));
		}

		versions.push(Version {
			major: comp.major,
			minor: comp.minor.unwrap_or(0),
			patch: comp.patch.unwrap_or(0),
			pre: Default::default(),
			build: Default::default(),
		});
	}

	let latest = versions.iter().max().ok_or_else(|| "No versions found".to_string())?;
	Ok(latest.clone())
}
