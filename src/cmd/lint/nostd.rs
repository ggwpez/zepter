// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{
	cmd::{lint::AutoFixer, resolve_dep, CargoArgs, GlobalArgs},
	grammar::plural,
	log,
};
use cargo_metadata::{DependencyKind, Package};
use std::{
	collections::{btree_map::Entry, BTreeMap},
	fs::canonicalize,
};

#[derive(Debug, clap::Parser)]
pub struct NoStdCmd {
	#[clap(subcommand)]
	sub: NoStdSubCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum NoStdSubCmd {
	/// Default features of no-std dependencies are disabled if the crate itself supports no-std.
	#[clap(name = "default-features-of-nostd-dependencies-disabled")]
	DefaultFeaturesDisabled(DefaultFeaturesDisabledCmd),
}

#[derive(Debug, clap::Parser)]
pub struct DefaultFeaturesDisabledCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,

	/// Whether to fix the issues.
	#[clap(long, short)]
	fix: bool,
}

impl NoStdCmd {
	pub(crate) fn run(&self, global: &GlobalArgs) -> Result<(), String> {
		match &self.sub {
			NoStdSubCmd::DefaultFeaturesDisabled(cmd) => cmd.run(global),
		}
	}
}

impl DefaultFeaturesDisabledCmd {
	pub(crate) fn run(&self, g: &GlobalArgs) -> Result<(), String> {
		let meta = self.cargo_args.clone().with_workspace(true).load_metadata()?;
		let pkgs = &meta.packages;
		let mut cache = BTreeMap::new();
		let mut autofixer = BTreeMap::new();
		let mut issues = 0;
		// Dir that we are allowed to write to.
		let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).unwrap();

		for lhs in pkgs.iter() {
			// check if lhs supports no-std builds
			if !Self::supports_nostd(g, lhs, &mut cache)? {
				continue;
			}

			for dep in lhs.dependencies.iter() {
				if dep.kind != DependencyKind::Normal {
					continue;
				}

				let Some(rhs) = resolve_dep(lhs, dep, &meta) else { continue };

				if !Self::supports_nostd(g, &rhs.pkg, &mut cache)? {
					continue;
				}

				if !dep.uses_default_features {
					continue;
				}

				println!(
					"Default features not disabled for dependency: {} -> {}",
					lhs.name, rhs.pkg.name
				);

				let fixer = match autofixer.entry(lhs.manifest_path.clone()) {
					Entry::Occupied(e) => e.into_mut(),
					Entry::Vacant(e) => {
						let krate_path =
							canonicalize(lhs.manifest_path.clone().into_std_path_buf()).unwrap();

						if !krate_path.starts_with(&allowed_dir) {
							return Err(format!("Cannot write to path: {}", krate_path.display()))
						}
						e.insert(AutoFixer::from_manifest(&lhs.manifest_path)?)
					},
				};

				fixer.disable_default_features(&rhs.name())?;
				issues += 1;
			}
		}

		let s = plural(autofixer.len());
		print!("Found {} issue{} in {} crate{s} ", issues, plural(issues), autofixer.len());
		if self.fix {
			for (_, fixer) in autofixer.iter_mut() {
				fixer.save()?;
			}
			println!("and fixed all of them.");
			Ok(())
		} else {
			println!("and fixed none. Re-run with --fix to apply fixes.");
			Err("Several issues were not fixed.".to_string())
		}
	}

	fn supports_nostd(
		g: &GlobalArgs,
		krate: &Package,
		cache: &mut BTreeMap<String, bool>,
	) -> Result<bool, String> {
		log::debug!("Checking if crate supports no-std: {}", krate.name);
		if let Some(res) = cache.get(krate.manifest_path.as_str()) {
			return Ok(*res)
		}

		// try to find the lib.rs
		let krate_root = krate
			.manifest_path
			.parent()
			.ok_or_else(|| format!("Could not find parent of manifest: {}", krate.manifest_path))?;
		let lib_rs = krate_root.join("src/lib.rs");

		if !lib_rs.exists() {
			return Ok(false)
		}
		let content = std::fs::read_to_string(&lib_rs)
			.map_err(|e| format!("Could not read lib.rs: {}", e))?;

		let ret = if content.contains("#![cfg_attr(not(feature = \"std\"), no_std)]") ||
			content.contains("#![no_std]")
		{
			if content.contains("\n#![cfg(") {
				println!(
					"{}: Crate may unexpectedly pull in libstd: {}",
					g.yellow("WARN"),
					krate.name
				);
			}
			log::debug!("Crate supports no-std: {} (path={})", krate.name, krate.manifest_path);
			true
		} else {
			false
		};

		cache.insert(krate.manifest_path.as_str().into(), ret);
		Ok(ret)
	}
}
