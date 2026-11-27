// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{
	cmd::{CargoArgs, CommandError, GlobalArgs},
	grammar::plural,
};
use camino::Utf8PathBuf;
use cargo_metadata::DependencyKind;
use itertools::Itertools;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Detect crates that needlessly list the same dependency both as `normal` and `dev` dependency.
#[derive(Debug, clap::Parser)]
pub struct DuplicateDepsCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,

	/// Show the path of the crate in the output.
	///
	/// This is useful for make it easier to fix, since many IDEs allow to click links in the
	/// console.
	#[clap(long)]
	show_paths: bool,
}

impl DuplicateDepsCmd {
	pub fn run(&self, _global: &GlobalArgs) -> Result<(), CommandError> {
		// To easily compare dependencies, we normalize them by removing the kind.
		fn without_kind(dep: &cargo_metadata::Dependency) -> cargo_metadata::Dependency {
			let mut dep = dep.clone();
			dep.kind = DependencyKind::Unknown;
			dep
		}

		let meta = self.cargo_args.load_metadata()?;

		let mut issues = BTreeMap::<(String, Utf8PathBuf), BTreeSet<String>>::new();

		for pkg in &meta.workspace_packages() {
			let deps: HashSet<_> = pkg
				.dependencies
				.iter()
				.filter(|d| d.kind == DependencyKind::Normal)
				.map(without_kind)
				.collect();

			let dev_deps: HashSet<_> = pkg
				.dependencies
				.iter()
				.filter(|d| d.kind == DependencyKind::Development)
				.map(without_kind)
				.collect();

			let intersection = deps.intersection(&dev_deps).sorted_by(|a, b| a.name.cmp(&b.name));
			for dep in intersection {
				issues
					.entry((pkg.name.to_string(), pkg.manifest_path.clone()))
					.or_default()
					.insert(dep.name.clone());
			}
		}

		if !issues.is_empty() {
			println!("Found {} crate{} with duplicated dependencies between [dependencies] and [dev-dependencies]", issues.len(), plural(issues.len()));
			for ((pkg, path), deps) in issues {
				let maybe_path = if self.show_paths {
					let rel = path.strip_prefix(&meta.workspace_root).map_err(|e| {
						format!("Manifest {path} is outside workspace {}: {e}", meta.workspace_root)
					})?;
					format!(" ({rel})")
				} else {
					"".to_string()
				};

				println!("  crate '{pkg}'{maybe_path}");

				for dep in deps {
					println!("    {dep}");
				}
			}

			return Err(CommandError::reported(1))
		}

		Ok(())
	}
}
