// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{
	cmd::{
		check_can_modify,
		transpose::{AutoFixer, Dep, Op, SourceLocationSelector, Version, VersionReq},
		CargoArgs, GlobalArgs,
	},
	grammar::{plural, plural_or},
	log, ErrToStr,
};
use cargo_metadata::Package;
use itertools::Itertools;
use std::collections::{BTreeMap, BTreeMap as Map, BTreeSet, HashMap};

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
		let maybe_rename = self.detect_rename(g, name, meta)?;
		let source_location = self.detect_source_location(meta, name)?;
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

		// Now create fixer for the root package
		let root_manifest_path = meta.workspace_root.join("Cargo.toml");
		fixers
			.entry("magic:workspace".to_string())
			.or_insert_with(|| (None, AutoFixer::from_manifest(&root_manifest_path).unwrap()));
		let (_, workspace_fixer) = fixers.get_mut("magic:workspace").unwrap();

		let mut dep = by_version.values().next().unwrap().first().unwrap().1.clone();
		dep.req = best_version.parse().unwrap();

		let location = match source_location {
			SourceLocationSelector::Local => {
				let Some(ref path) = dep.path else {
					unreachable!("Could not detect local source location for '{}'", name);
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
				if pkg.name == *skip_package {
					continue
				}
			}

			fixers.entry(pkg.name.clone()).or_insert_with(|| {
				(Some(pkg.clone()), AutoFixer::from_manifest(&pkg.manifest_path).unwrap())
			});
			let (_, fixer) = fixers.get_mut(&pkg.name).unwrap();
			// We can safely use the rename here, since we found it with `detect_rename`.
			let dep_name = dep.rename.as_ref().unwrap_or(&dep.name);
			if let Some(rename) = &maybe_rename {
				assert_eq!(rename, dep_name);
			}
			let Some(ref location) = source_location else {
				return Err("Could not determine source location".to_string());
			};

			if dep.uses_default_features != workspace_default_features_enabled {
				fixer.lift_dependency(
					dep_name,
					&dep.kind,
					Some(dep.uses_default_features),
					location,
				)?;
			} else {
				fixer.lift_dependency(dep_name, &dep.kind, None, location)?;
			}
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
	) -> Result<Option<SourceLocationSelector>, String> {
		let mut local = false;
		let mut remote = false;

		// TODO check that they all point to the same folder

		for pkg in meta.packages.iter() {
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
				"Dependency '{}' is used both locally and remotely. This cannot be fixed automatically.",
				name
			))
		} else if local {
			Ok(SourceLocationSelector::Local)
		} else if remote {
			Ok(SourceLocationSelector::Remote)
		} else {
			Error(format!(
				"Dependency '{}' is not used in the workspace. This cannot be fixed automatically.",
				name
			))
		}
	}

	fn detect_rename(
		&self,
		g: &GlobalArgs,
		name: &str,
		meta: &cargo_metadata::Metadata,
	) -> Result<Option<String>, String> {
		let mut renames = BTreeMap::<String, Vec<String>>::new();
		let mut unnrenamed = BTreeSet::<String>::new();

		for pkg in meta.packages.iter() {
			if let Some(skip_package) = &self.skip_package {
				if pkg.name == *skip_package {
					continue
				}
			}

			for dep in pkg.dependencies.iter() {
				if dep.name == name {
					if let Some(rename) = &dep.rename {
						if name == rename {
							log::warn!(
								"Dependency '{}' is renamed to itself in '{}'",
								name,
								pkg.name
							);
							unnrenamed.insert(pkg.name.clone());
						} else {
							renames
								.entry(dep.rename.clone().unwrap())
								.or_default()
								.push(pkg.name.clone());
						}
					} else {
						unnrenamed.insert(pkg.name.clone());
					}
				}
			}
		}

		// TODO write formatting function to also sort them
		if !renames.is_empty() && !unnrenamed.is_empty() {
			let mut err = String::new();
			for (rename, pkgs) in renames.iter() {
				let s = plural_or(pkgs.len(), " ");
				err += &format!(
					"{: >3} time{s}: {} from {}",
					pkgs.len(),
					g.bold(rename),
					pkgs.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
				);
				if pkgs.len() > 3 {
					err += &format!(", … ({} more)", pkgs.len() - 3);
				}
				err += "\n";
			}
			let s = plural_or(unnrenamed.len(), " ");
			err += &format!(
				"{: >3} time{s}: {} from {}",
				unnrenamed.len(),
				g.bold("no alias"),
				unnrenamed.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
			);
			if unnrenamed.len() > 3 {
				err += &format!(", … ({} more)", unnrenamed.len() - 3);
			}

			let renames_count = renames.values().map(|pkgs| pkgs.len()).sum::<usize>();
			Err(format!(
				"Dependency '{}' is used {} time{} with and {} time{} without an alias:\n\n{err}\n\nThis cannot be fixed automatically since it would break your code and configs.",
				g.bold(name),
				renames_count,
				plural(renames_count),
				unnrenamed.len(),
				plural(unnrenamed.len()),
			))
		} else if renames.is_empty() {
			return Ok(None)
		} else if renames.len() == 1 {
			Ok(Some(renames.keys().next().unwrap().clone()))
		} else {
			let mut err = String::new();
			for (rename, pkgs) in renames.iter() {
				let s = plural_or(pkgs.len(), " ");
				err += &format!(
					"{: >3} time{s}: {} from {}",
					pkgs.len(),
					g.bold(rename),
					pkgs.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
				);
				if pkgs.len() > 3 {
					err += &format!(", … ({} more)", pkgs.len() - 3);
				}
				err += "\n";
			}

			return Err(format!(
				"Dependency '{}' is used with {} conflicting aliases:\n\n{}\nThis cannot be fixed automatically since it would break your code and configs.",
				g.bold(name),
				renames.len(),
				err
			))
		}
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
