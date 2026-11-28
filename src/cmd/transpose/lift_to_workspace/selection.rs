// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{LiftToWorkspaceCmd, SourceLocationSelector};
use crate::ErrToStr;
use cargo_metadata::{Dependency as Dep, Metadata, Package};
use semver::VersionReq;
use std::collections::{BTreeMap as Map, BTreeSet, HashMap};

impl LiftToWorkspaceCmd {
	pub(super) fn select_dependencies<'a>(
		&self,
		meta: &'a Metadata,
	) -> Result<BTreeSet<&'a str>, String> {
		let mut regex_lookup = Map::new();
		for filter in self.dependencies.iter() {
			if let Some(regex) = filter.strip_prefix("regex:") {
				regex_lookup.insert(regex, regex::Regex::new(regex).err_to_str()?);
			}
		}

		let mut dependencies = BTreeSet::new();
		for dep in meta.packages.iter().flat_map(|pkg| &pkg.dependencies) {
			if !regex_lookup.values().any(|r| r.is_match(&dep.name)) &&
				!self.dependencies.contains(&dep.name)
			{
				continue
			}
			let matches_location = match self.source_location {
				Some(SourceLocationSelector::Local) => dep.path.is_some(),
				Some(SourceLocationSelector::Remote) => dep.path.is_none(),
				None => true,
			};
			if matches_location {
				dependencies.insert(dep.name.as_str());
			}
		}
		Ok(dependencies)
	}

	/// Index what versions of a crate are used in the workspace.
	pub(super) fn build_version_index(
		meta: &Metadata,
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

	pub(super) fn detect_source_location(
		&self,
		meta: &Metadata,
		name: &str,
	) -> Result<SourceLocationSelector, String> {
		let mut local = false;
		let mut remote = false;

		// TODO check that they all point to the same folder
		for pkg in meta.packages.iter() {
			if self.skip_package.as_ref().is_some_and(|skip| pkg.name == *skip) {
				continue
			}
			for dep in pkg.dependencies.iter().filter(|dep| dep.name == name) {
				if dep.path.is_some() {
					local = true;
				} else {
					remote = true;
				}
			}
		}

		match (local, remote) {
			(true, true) => Err(format!(
				"Dependency '{name}' is used both locally and remotely. This cannot be fixed automatically."
			)),
			(true, false) => Ok(SourceLocationSelector::Local),
			(false, true) => Ok(SourceLocationSelector::Remote),
			(false, false) => Err(format!(
				"Dependency '{name}' is not used in the workspace. This cannot be fixed automatically."
			)),
		}
	}
}
