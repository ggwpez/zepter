// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{lint::DepKind, GlobalArgs};
use crate::{autofix::*, cmd::resolve_dep, grammar::*, log};

use cargo_metadata::{Dependency as Dep, DependencyKind, Package};
use itertools::Itertools;
use semver::{Op, Version, VersionReq};
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
			DependencySubCmd::StripDevFeatures(cmd) => cmd.run(global),
		}
	}
}

#[derive(Debug, clap::Subcommand)]
pub enum DependencySubCmd {
	#[clap(alias = "lift", alias = "l")]
	LiftToWorkspace(LiftToWorkspaceCmd),
	/// Strip out dev dependencies.
	StripDevFeatures(StripDevDepsCmd),
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

#[derive(Debug, clap::Parser)]
pub struct StripDevDepsCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	/// Only consider these packages.
	#[clap(long, short = 'p', value_delimiter = ',', verbatim_doc_comment)]
	packages: Option<Vec<String>>,
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
			for (version, pkgs) in
				by_version.iter().sorted_by_key(|(v, pkgs)| (pkgs.len(), v.to_string())).rev()
			{
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
				Err(_e) => {
					log::warn!("Could not find determine latest common version: {}", _e);
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
		let _found: usize = by_version.values().map(Vec::len).sum();

		log::info!(
			"Selected '{} {}' for lift up ({} occurrence{})", //: N={}, D={}, B={})",
			&self.dependency,
			&version,
			_found,
			crate::grammar::plural(_found),
		);

		let mut fixers = Map::new();
		for (pkg, dep) in by_version.values().flatten() {
			let krate_path = canonicalize(pkg.manifest_path.clone().into_std_path_buf()).unwrap();
			fixers
				.entry(pkg.name.clone())
				.or_insert_with(|| AutoFixer::from_manifest(&krate_path).unwrap());
			let fixer = fixers.get_mut(&pkg.name).unwrap();

			fixer.lift_dependency(&dep.name, None).unwrap(); // TODO
		}

		for fixer in fixers.values_mut() {
			fixer.save().unwrap();
		}

		// Now create fixer for the root package
		let root_manifest_path = meta.workspace_root.join("Cargo.toml");
		let mut fixer = AutoFixer::from_manifest(&root_manifest_path.into_std_path_buf()).unwrap();
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
			return Err(format!("Invalid version requirement: '{}'", req))
		}
		let comp = req.comparators.first().unwrap();
		if comp.op != Op::Caret {
			return Err(format!("Only caret is supported, but got: '{}'", req))
		}
		if !comp.pre.is_empty() {
			return Err(format!("Pre-release versions are not supported: '{}'", req))
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

impl StripDevDepsCmd {
	pub fn run(&self, g: &GlobalArgs) {
		g.warn_unstable();

		let mut args = self.cargo_args.clone();
		args.workspace = true;
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		let kind = DependencyKind::Development;
		// Allowed dir that we can write to.
		let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).unwrap();

		for name in self.packages.iter().flatten() {
			if !meta.packages.iter().any(|p| p.name == *name) {
				panic!("Could not find package named '{}'", g.red(name));
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
					panic!("Could not resolve dependency '{}'", g.red(&dep.name));
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
