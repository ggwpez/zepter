// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{
	cmd::{
		check_can_modify,
		transpose::{AutoFixer, Dep, Op, Version, VersionReq},
		CargoArgs, GlobalArgs,
	},
	grammar::{plural, plural_or},
	log, ErrToStr,
};

use cargo_metadata::Package;
use itertools::Itertools;
use std::collections::{BTreeMap as Map, BTreeSet, HashMap};

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
	#[clap(long, alias = "version-resolver", value_enum, default_value_t = VersionSelectorMode::Unambiguous, requires_if("exact", "exact_version"))]
	version_selector: VersionSelectorMode,

	/// Optionally only check dependencies with this source location.
	#[clap(long, value_enum)]
	source_location: Option<SourceLocationSelector>,

	/// The exact version to use for the whole workspace.
	#[clap(long)]
	exact_version: Option<String>,

	/// Ignore errors and continue with the next dependency.
	#[clap(long)]
	ignore_errors: bool,
}

/// How to determine which version to use for the whole workspace.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum VersionSelectorMode {
	/// The version must be unambiguous - eg. there is only one version in the workspace.
	Unambiguous,
	/// A specific version.
	Exact,
	/// The latest version that was seen in the workspace.
	///
	/// The highest version number that it found.
	Highest,
}

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum SourceLocationSelector {
	/// The dependency is referenced via a `path`.
	Local,
	/// Either git or a registry.
	Remote,
}

impl LiftToWorkspaceCmd {
	pub fn run(&self, g: &GlobalArgs) -> Result<(), String> {
		g.warn_unstable();
		self.validate_args()?;

		let meta = self.cargo_args.clone().with_workspace(true).load_metadata()?;
		let mut fixers = Map::new();

		// TODO optimize to not be O^3
		let mut dependencies = BTreeSet::<&str>::new();
		let mut regex_lookup = Map::new();
		for filter in self.dependencies.iter() {
			if let Some(regex) = filter.strip_prefix("regex:") {
				regex_lookup.insert(regex, regex::Regex::new(regex).err_to_str()?);
			}
		}

		for pkg in meta.packages.iter() {
			for dep in pkg.dependencies.iter() {
				if !regex_lookup.values().any(|r| r.is_match(&dep.name)) &&
					!self.dependencies.contains(&dep.name)
				{
					continue;
				}

				if let Some(location_filter) = &self.source_location {
					let is_local = dep.path.is_some();
					match location_filter {
						SourceLocationSelector::Local if !is_local => continue,
						SourceLocationSelector::Remote if is_local => continue,
						_ => (),
					}
				}

				dependencies.insert(&dep.name);
			}
		}

		log::info!("Scanning for {} dependencies in the workspace.", dependencies.len());
		for dep in &dependencies {
			match self.run_for_dependency(g, &meta, dep, &mut fixers) {
				Ok(()) => (),
				Err(e) if self.ignore_errors => {
					log::error!("Failed to lift up '{}': {}", dep, e);
				},
				Err(e) => return Err(format!("Failed to lift up '{}': {}", dep, e)),
			}
		}

		self.try_apply_changes(&mut fixers)
	}

	fn validate_args(&self) -> Result<(), String> {
		if self.exact_version.is_some() && self.version_selector != VersionSelectorMode::Exact {
			return Err("Cannot use --exact-version without --version-selector=exact".to_string())
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
			log::info!("Modified {} manifest{}.", modified, plural(modified));
			Ok(())
		}
	}

	fn run_for_dependency(
		&self,
		g: &GlobalArgs,
		meta: &cargo_metadata::Metadata,
		name: &str,
		fixers: &mut Map<String, (Option<Package>, AutoFixer)>,
	) -> Result<(), String> {
		let by_version = Self::build_version_index(meta, name);
		let versions = by_version.keys().collect::<Vec<_>>();
		let best_version = self.find_best_version(g, name, &versions, &by_version)?;

		let mut all_use_default_features = true;
		for (pkg, dep) in by_version.values().flatten() {
			if !check_can_modify(&meta.workspace_root, &pkg.manifest_path)? {
				continue
			}

			all_use_default_features &= dep.uses_default_features;
		}

		// We default in the workspace to enabling them if all packages use them but otherwise turn
		// them off.
		let workspace_default_features_enabled = all_use_default_features;

		for (pkg, dep) in by_version.values().flatten() {
			if !check_can_modify(&meta.workspace_root, &pkg.manifest_path)? {
				continue
			}

			fixers.entry(pkg.name.clone()).or_insert_with(|| {
				(Some(pkg.clone()), AutoFixer::from_manifest(&pkg.manifest_path).unwrap())
			});
			let (_, fixer) = fixers.get_mut(&pkg.name).unwrap();

			if dep.uses_default_features != workspace_default_features_enabled {
				fixer.lift_dependency(&dep.name, Some(dep.uses_default_features))?;
			} else {
				fixer.lift_dependency(&dep.name, None)?;
			}
		}

		// Now create fixer for the root package
		let root_manifest_path = meta.workspace_root.join("Cargo.toml");
		fixers
			.entry("magic:workspace".to_string())
			.or_insert_with(|| (None, AutoFixer::from_manifest(&root_manifest_path).unwrap()));
		let (_, workspace_fixer) = fixers.get_mut("magic:workspace").unwrap();

		let mut dep = by_version.values().next().unwrap().first().unwrap().1.clone();
		dep.req = best_version.parse().unwrap();

		workspace_fixer.add_workspace_dep(&dep, workspace_default_features_enabled)?;

		#[cfg(feature = "logging")]
		{
			let total_changes = by_version.values().map(|v| v.len()).sum::<usize>();
			let v = format!("{} {}", name, &best_version);
			log::info!(
				"Selected {} for lift up in {} crate{}.",
				g.bold(&v),
				total_changes,
				plural(total_changes)
			);
		}

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
		let found = match self.version_selector {
			VersionSelectorMode::Exact => self.exact_version.clone().expect("Checked by clippy"),
			VersionSelectorMode::Highest => try_find_latest(by_version.keys())?,
			VersionSelectorMode::Unambiguous => {
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
						Ok(latest) => latest,
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

fn try_find_latest<'a, I: Iterator<Item = &'a VersionReq>>(reqs: I) -> Result<String, String> {
	let reqs = reqs.collect::<Vec<_>>();

	if reqs.is_empty() {
		return Err("No versions found".to_string())
	}
	if reqs.iter().all(|r| r == &reqs[0]) {
		return Ok(reqs[0].to_string())
	}

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
	Ok(latest.clone().to_string())
}
