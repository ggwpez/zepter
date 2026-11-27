// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::LiftToWorkspaceCmd;
use crate::{cmd::GlobalArgs, grammar::plural_or, log};
use cargo_metadata::{Dependency as Dep, Package};
use itertools::Itertools;
use semver::{Op, Version, VersionReq};
use std::collections::HashMap;

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
	pub(super) fn find_best_version(
		&self,
		g: &GlobalArgs,
		name: &str,
		versions: &[&VersionReq],
		by_version: &HashMap<VersionReq, Vec<(Package, Dep)>>,
	) -> Result<String, String> {
		let found = match self.version_selector {
			VersionSelectorMode::Exact => self.exact_version.clone().ok_or_else(|| {
				"Missing --exact-version for --version-selector=exact".to_string()
			})?,
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
							log::warn!(
								target: super::LOG_TARGET,
								"Could not find determine latest common version: {}",
								_e
							);
							"version".to_string()
						},
					};

					let hint = format!("cargo upgrade -p {name}@{version_hint}");
					return Err(format!(
						"\nFound {} different versions of '{}' in the workspace:\n\n{err}\nHint: {}\n",
						versions.len(),
						name,
						g.bold(&hint),
					))
				} else {
					versions.first().ok_or_else(|| "No versions found".to_string())?.to_string()
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
			return Err(format!("Invalid version requirement: '{req}'"))
		}
		let comp = req.comparators.first().unwrap();
		if comp.op != Op::Caret {
			return Err(format!("Only caret is supported, but got: '{req}'"))
		}
		if !comp.pre.is_empty() {
			return Err(format!("Pre-release versions are not supported: '{req}'"))
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
