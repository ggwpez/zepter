// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{
	cmd::{
		transpose::{canonicalize, AutoFixer, Dep, Op, Version, VersionReq},
		CargoArgs, GlobalArgs,
	},
	grammar::{plural, plural_or},
};
use cargo_metadata::Package;
use itertools::Itertools;
use std::collections::{BTreeMap as Map, HashMap};

/// Lift up a dependency to the workspace and reference it from all packages.
#[derive(Debug, clap::Parser)]
pub struct LiftToWorkspaceCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,

	#[clap(index(1))]
	dependencies: Vec<String>,

	/// Instead of dry-running, actually modify the files.
	#[clap(long)]
	fix: bool,

	/// How to determine which version to use for the whole workspace.
	#[clap(long, value_enum, default_value_t = VersionResolveMode::Unambiguous, requires_if("exact", "exact_version"))]
	version_resolver: VersionResolveMode,

	/// The exact version to use for the whole workspace.
	#[clap(long)]
	exact_version: Option<String>,
}

/// How to determine which version to use for the whole workspace.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum VersionResolveMode {
	/// The version must be unambiguous - eg. there is only one version in the workspace.
	Unambiguous,
	/// A specific version.
	Exact,
	/// The latest version that was seen in the workspace.
	///
	/// This is *not* the latest version from crates-io.
	Latest,
}

impl LiftToWorkspaceCmd {
	pub fn run(&self, g: &GlobalArgs) -> Result<(), String> {
		g.warn_unstable();
		self.validate_args()?;

		let meta = self.cargo_args.clone().with_workspace(true).load_metadata()?;
		let mut fixers = Map::new();

		for dep in &self.dependencies {
			self.run_for_dependency(g, &meta, dep, &mut fixers)?;
		}

		self.try_apply_changes(&mut fixers)
	}

	fn validate_args(&self) -> Result<(), String> {
		if self.exact_version.is_some() && self.version_resolver != VersionResolveMode::Exact {
			return Err("Cannot use --exact-version without --version-resolver=exact".to_string())
		}
		Ok(())
	}

	fn try_apply_changes(
		&self,
		fixers: &mut Map<String, (Option<Package>, AutoFixer)>,
	) -> Result<(), String> {
		let mut modified = 0;
		for (_pkg, fixer) in fixers.values_mut() {
			if !fixer.modified() {
				continue
			}

			modified += 1;
			if self.fix {
				fixer.save()?;
			} else if let Some(_pkg) = _pkg {
				log::debug!("Would modify {:?}", _pkg.name);
			} else {
				log::debug!("Would modify the workspace");
			}
		}
		if modified > 0 && !self.fix {
			let s = plural(modified);
			Err(format!(
				"Held back modifications to {modified} file{s}. Re-run with --fix to apply."
			))
		} else {
			Ok(())
		}
	}

	fn run_for_dependency(
		&self,
		g: &GlobalArgs,
		meta: &cargo_metadata::Metadata,
		dep: &str,
		fixers: &mut Map<String, (Option<Package>, AutoFixer)>,
	) -> Result<(), String> {
		let by_version = Self::build_version_index(meta, dep);
		let versions = by_version.keys().collect::<Vec<_>>();
		let best_version = self.find_best_version(g, dep, &versions, &by_version)?;

		log::info!(
			"Selected '{} {}'", //: N={}, D={}, B={})",
			dep,
			&best_version,
		);

		for (pkg, dep) in by_version.values().flatten() {
			let krate_path = canonicalize(pkg.manifest_path.clone().into_std_path_buf()).unwrap();
			let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).unwrap();

			if !krate_path.starts_with(&allowed_dir) {
				log::info!(
					"Skipping path outside of the workspace: {:?} (not in {:?})",
					krate_path.display(),
					allowed_dir.display()
				);
				continue
			}

			fixers.entry(pkg.name.clone()).or_insert_with(|| {
				(Some(pkg.clone()), AutoFixer::from_manifest(&krate_path).unwrap())
			});
			let (_, fixer) = fixers.get_mut(&pkg.name).unwrap();

			let default_feats = dep.uses_default_features.then_some(true);
			fixer.lift_dependency(&dep.name, default_feats)?;
		}

		// Now create fixer for the root package
		let root_manifest_path = meta.workspace_root.join("Cargo.toml");
		fixers.entry("workspace".to_string()).or_insert_with(|| {
			(None, AutoFixer::from_manifest(&root_manifest_path.into_std_path_buf()).unwrap())
		});
		let (_, workspace_fixer) = fixers.get_mut("workspace").unwrap();

		let mut dep = by_version.values().next().unwrap().first().unwrap().1.clone();
		dep.req = best_version.parse().unwrap();
		// We always add `default-features = false` into the workspace:
		workspace_fixer.add_workspace_dep(&dep, false)?;

		Ok(())
	}

	/// Index what versions of a crate are used in the workspace.
	fn build_version_index(
		meta: &cargo_metadata::Metadata,
		name: &str,
	) -> HashMap<VersionReq, Vec<(Package, Dep)>> {
		let mut by_version = HashMap::<VersionReq, Vec<(Package, Dep)>>::new();
		for pkg in meta.packages.iter() {
			for dep in pkg.dependencies.iter() {
				if dep.name != name {
					continue
				}

				by_version.entry(dep.req.clone()).or_default().push((pkg.clone(), dep.clone()));
			}
		}
		by_version
	}

	fn find_best_version(
		&self,
		g: &GlobalArgs,
		name: &str,
		versions: &[&VersionReq],
		by_version: &HashMap<VersionReq, Vec<(Package, Dep)>>,
	) -> Result<String, String> {
		let found = match self.version_resolver {
			VersionResolveMode::Exact => self.exact_version.clone().expect("Checked by clippy"),
			VersionResolveMode::Latest => try_find_latest(by_version.keys())?.to_string(),
			VersionResolveMode::Unambiguous => {
				if versions.len() > 1 {
					let str_width = versions.iter().map(|v| v.to_string().len()).max().unwrap();
					let mut err = String::new();
					// iter by descending frequency
					for (version, pkgs) in by_version
						.iter()
						.sorted_by_key(|(v, pkgs)| (pkgs.len(), v.to_string()))
						.rev()
					{
						let ddd = if pkgs.len() > 3 { ", …" } else { "" };
						let s = plural_or(pkgs.len(), " ");
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

					let hint = format!("cargo upgrade -p {}@{version_hint}", name);
					return Err(format!(
						"\nFound {} different versions of '{}' in the workspace:\n\n{err}\nHint: {}\n",
						versions.len(),
						name,
						g.bold(&hint),
					))
				} else {
					versions.first().unwrap().to_string()
				}
			},
		};
		Ok(found)
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
