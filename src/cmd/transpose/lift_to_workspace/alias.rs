// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::LiftToWorkspaceCmd;
use crate::{
	cmd::GlobalArgs,
	grammar::{plural, plural_or},
	log,
};
use std::collections::{BTreeMap, BTreeSet};

impl LiftToWorkspaceCmd {
	pub(super) fn detect_rename(
		&self,
		g: &GlobalArgs,
		name: &str,
		meta: &cargo_metadata::Metadata,
	) -> Result<Option<String>, String> {
		let mut renames = BTreeMap::<String, Vec<String>>::new();
		let mut unnrenamed = BTreeSet::<String>::new();

		for pkg in meta.packages.iter() {
			if let Some(skip_package) = &self.skip_package {
				if pkg.name == skip_package {
					continue
				}
			}

			for dep in pkg.dependencies.iter() {
				if dep.name == name {
					if let Some(rename) = &dep.rename {
						if name == rename {
							log::warn!(
								target: super::LOG_TARGET,
								"Dependency '{}' is renamed to itself in '{}'",
								name,
								pkg.name
							);
							unnrenamed.insert(pkg.name.to_string());
						} else {
							renames
								.entry(dep.rename.clone().unwrap())
								.or_default()
								.push(pkg.name.to_string());
						}
					} else {
						unnrenamed.insert(pkg.name.to_string());
					}
				}
			}
		}

		Self::resolve_alias(g, name, &renames, &unnrenamed)
	}

	fn resolve_alias(
		g: &GlobalArgs,
		name: &str,
		renames: &BTreeMap<String, Vec<String>>,
		unnrenamed: &BTreeSet<String>,
	) -> Result<Option<String>, String> {
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
			Ok(None)
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

			Err(format!(
				"Dependency '{}' is used with {} conflicting aliases:\n\n{}\nThis cannot be fixed automatically since it would break your code and configs.",
				g.bold(name),
				renames.len(),
				err
			))
		}
	}
}
