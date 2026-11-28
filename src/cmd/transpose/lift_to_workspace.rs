// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod alias;
mod version;

use crate::{
	cmd::{
		check_can_modify,
		transpose::{AutoFixer, SourceLocationSelector},
		CargoArgs, GlobalArgs,
	},
	grammar::plural,
	log, ErrToStr,
};
use cargo_metadata::{Dependency as Dep, Metadata, Package};
use semver::VersionReq;
use std::collections::{BTreeMap as Map, BTreeSet, HashMap};

use version::VersionSelectorMode;

#[cfg(feature = "logging")]
const LOG_TARGET: &str = module_path!();

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

	/// Do not try to modify this package.
	#[clap(long)]
	skip_package: Option<String>,

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

impl LiftToWorkspaceCmd {
	pub fn run(&self, g: &GlobalArgs) -> Result<(), String> {
		g.warn_unstable();
		self.validate_args()?;

		let meta = self.cargo_args.clone().with_workspace(true).load_metadata()?;
		let mut fixers = Map::new();

		let dependencies = self.select_dependencies(&meta)?;

		log::info!("Scanning for {} dependencies in the workspace.", dependencies.len());
		for dep in &dependencies {
			match self.run_for_dependency(g, &meta, dep, &mut fixers) {
				Ok(()) => (),
				Err(e) if self.ignore_errors => {
					let _ = e;
					log::error!("Failed to lift up '{}': {}", dep, e);
				},
				Err(e) => return Err(format!("Failed to lift up '{dep}': {e}")),
			}
		}

		self.try_apply_changes(&mut fixers)
	}

	fn select_dependencies<'a>(&self, meta: &'a Metadata) -> Result<BTreeSet<&'a str>, String> {
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

		Ok(dependencies)
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
		let maybe_rename = self.detect_rename(g, name, meta)?;
		let source_location = self.detect_source_location(meta, name)?;
		let by_version = Self::build_version_index(meta, name);
		let versions = by_version.keys().collect::<Vec<_>>();
		let best_version = self.find_best_version(g, name, &versions, &by_version)?;

		let all_use_default_features = Self::workspace_default_features(meta, &by_version)?;

		// We default in the workspace to enabling them if all packages use them but otherwise turn
		// them off.
		let workspace_default_features_enabled = all_use_default_features;

		// Now create fixer for the root package
		let root_manifest_path = meta.workspace_root.join("Cargo.toml");
		let (_, workspace_fixer) = match fixers.entry("magic:workspace".to_string()) {
			std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
			std::collections::btree_map::Entry::Vacant(entry) =>
				entry.insert((None, AutoFixer::from_manifest(&root_manifest_path)?)),
		};

		let mut dep = by_version
			.values()
			.flatten()
			.next()
			.ok_or_else(|| format!("No dependencies found for '{name}'"))?
			.1
			.clone();
		dep.req = best_version
			.parse()
			.map_err(|e| format!("Invalid version requirement '{best_version}': {e}"))?;

		let location = match source_location {
			SourceLocationSelector::Local => {
				let Some(ref path) = dep.path else {
					return Err(format!("Could not detect local source location for '{name}'"))
				};
				let relative = path.strip_prefix(&meta.workspace_root).unwrap_or_else(|_| {
					log::warn!("Dependency '{}' is not in the workspace root", name);
					path
				});
				Some(relative.to_string())
			},
			SourceLocationSelector::Remote => None,
		};

		workspace_fixer.add_workspace_dep(
			&dep,
			maybe_rename.as_deref(),
			workspace_default_features_enabled,
			location.as_deref(),
		)?;

		for (pkg, dep) in by_version.values().flatten() {
			if !check_can_modify(&meta.workspace_root, &pkg.manifest_path)? {
				continue
			}
			if let Some(skip_package) = &self.skip_package {
				if pkg.name == skip_package {
					continue
				}
			}

			let (_, fixer) = match fixers.entry(pkg.name.to_string()) {
				std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
				std::collections::btree_map::Entry::Vacant(entry) =>
					entry.insert((Some(pkg.clone()), AutoFixer::from_manifest(&pkg.manifest_path)?)),
			};
			Self::configure_member_dependency(
				fixer,
				dep,
				name,
				maybe_rename.as_ref(),
				&source_location,
				workspace_default_features_enabled,
			)?;
		}

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

	fn workspace_default_features(
		meta: &Metadata,
		by_version: &HashMap<VersionReq, Vec<(Package, Dep)>>,
	) -> Result<bool, String> {
		let mut all_use_default_features = true;
		for (pkg, dep) in by_version.values().flatten() {
			if !check_can_modify(&meta.workspace_root, &pkg.manifest_path)? {
				continue
			}

			all_use_default_features &= dep.uses_default_features;
		}

		Ok(all_use_default_features)
	}

	fn configure_member_dependency(
		fixer: &mut AutoFixer,
		dep: &Dep,
		name: &str,
		maybe_rename: Option<&String>,
		location: &SourceLocationSelector,
		workspace_default_features_enabled: bool,
	) -> Result<(), String> {
		// We can safely use the rename here, since we found it with `detect_rename`.
		let dep_name = dep.rename.as_ref().unwrap_or(&dep.name);
		if maybe_rename.is_some_and(|rename| rename != dep_name) {
			return Err(format!("Dependency '{name}' has conflicting alias '{dep_name}'"))
		}

		if dep.uses_default_features != workspace_default_features_enabled {
			fixer.lift_dependency(dep_name, &dep.kind, Some(dep.uses_default_features), location)
		} else {
			fixer.lift_dependency(dep_name, &dep.kind, None, location)
		}
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

	fn detect_source_location(
		&self,
		meta: &cargo_metadata::Metadata,
		name: &str,
	) -> Result<SourceLocationSelector, String> {
		let mut local = false;
		let mut remote = false;

		// TODO check that they all point to the same folder

		for pkg in meta.packages.iter() {
			if let Some(skip_package) = &self.skip_package {
				if pkg.name == skip_package {
					continue
				}
			}

			for dep in pkg.dependencies.iter() {
				if dep.name == name {
					if dep.path.is_some() {
						local = true;
					} else {
						remote = true;
					}
				}
			}
		}

		if local && remote {
			Err(format!(
				"Dependency '{name}' is used both locally and remotely. This cannot be fixed automatically."
			))
		} else if local {
			Ok(SourceLocationSelector::Local)
		} else if remote {
			Ok(SourceLocationSelector::Remote)
		} else {
			Err(format!(
				"Dependency '{name}' is not used in the workspace. This cannot be fixed automatically."
			))
		}
	}
}
