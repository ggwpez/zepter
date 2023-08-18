// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Format features in the crate manifest.

use crate::{autofix::*, grammar::*, log};
use std::{fs::canonicalize, path::PathBuf};

use super::GlobalArgs;

/// Format the features in your manifest files.
#[derive(Debug, clap::Parser)]
pub struct FormatCmd {
	#[clap(subcommand)]
	subcommand: SubCommand,
}

/// Sub-commands of the [Format](FormatCmd) command.
#[derive(Debug, clap::Subcommand)]
pub enum SubCommand {
	Features(FormatFeaturesCmd),
}

/// Format the content of each feature in the crate manifest.
#[derive(Debug, clap::Parser)]
pub struct FormatFeaturesCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	/// Paths that are allowed to be modified by the formatter.
	#[clap(long)]
	modify_paths: Vec<PathBuf>,

	/// Fix the offending features by sorting them.
	#[clap(long)]
	fix: bool,

	/// Also print the offending features per crate.
	#[clap(long)]
	print_features: bool,

	/// Also print the paths of the offending Cargo.toml files.
	#[clap(long)]
	print_paths: bool,
}

impl FormatCmd {
	pub fn run(&self, global: &GlobalArgs) {
		match &self.subcommand {
			SubCommand::Features(cmd) => cmd.run(global),
		}
	}
}

impl FormatFeaturesCmd {
	pub fn run(&self, global: &GlobalArgs) {
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		// modifications are only allowed in this dir:
		let allowed_dir = canonicalize(&self.cargo_args.manifest_path).unwrap();
		let allowed_dir = allowed_dir.parent().unwrap();
		let mut offenders = Vec::new();

		for pkg in meta.packages.iter() {
			let path = canonicalize(pkg.manifest_path.clone().into_std_path_buf()).unwrap();

			let fixer = AutoFixer::from_manifest(&path).unwrap();
			let unsorted = fixer.check_sorted_all_features();
			if unsorted.is_empty() {
				continue
			}
			offenders.push((path, &pkg.name, unsorted));
		}
		if offenders.is_empty() {
			log::info!(
				"Checked {} crate{}: all formatted",
				meta.packages.len(),
				plural(meta.packages.len())
			);
			return
		}

		let mut fixed = 0;
		println!(
			"Found {} crate{} with unsorted features:",
			global.red(&offenders.len().to_string()),
			plural(offenders.len())
		);
		for (path, pkg, features) in offenders.iter() {
			// trim of the allowed_dir, if possible:
			let psuffix =
				self.print_paths.then(|| format!(" {}", path.display())).unwrap_or_default();
			let feats = self
				.print_features
				.then(|| format!(" ({})", features.join(", ")))
				.unwrap_or_default();
			println!("  {}{}{}", global.bold(pkg), psuffix, feats);

			if !self.fix {
				continue
			}

			let can_modify = path.starts_with(allowed_dir) ||
				self.modify_paths.iter().any(|p| path.starts_with(p));
			if !can_modify {
				log::warn!(
					"Not allowed to modify {} outside of: {}",
					path.display(),
					allowed_dir.display()
				);
				continue
			}

			let mut fixer = AutoFixer::from_manifest(path).unwrap();
			fixer.sort_all_features().unwrap();
			fixer.save().unwrap();
			fixed += 1;
		}

		log::info!(
			"Checked {} crate{}: {} with unsorted features",
			meta.packages.len(),
			plural(meta.packages.len()),
			offenders.len()
		);

		if self.fix {
			println!(
				"Fixed {} crate{} with unsorted features",
				global.green(&fixed.to_string()),
				plural(fixed)
			);

			if fixed != offenders.len() {
				log::error!(
					"ASSERT FAILED THIS IS A BUG: {} crate{} could not be fixed",
					offenders.len() - fixed,
					plural(offenders.len() - fixed)
				);
			}
			std::process::exit(0);
		}

		std::process::exit(1);
	}
}
