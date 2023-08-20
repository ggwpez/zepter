// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Format features in the crate manifest.

use crate::{autofix::*, cmd::parse_key_val, grammar::*, log};
use std::{collections::BTreeMap as Map, fs::canonicalize, path::PathBuf, str::FromStr};

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

	/// Set the formatting mode for a specific feature.
	///
	/// Can be specified multiple times. Example:
	/// `--mode-per-feature default:sort,default:canonicalize`
	#[clap(long, value_name = "FEATURE:MODE", value_parser = parse_key_val::<String, Mode>, value_delimiter = ',', verbatim_doc_comment)]
	mode_per_feature: Option<Vec<(String, Mode)>>,

	/// Ignore a specific feature across all crates.
	///
	/// This is equivalent to `--mode-per-feature FEATURE:none`.
	#[clap(long, value_name = "FEATURE", value_delimiter = ',', verbatim_doc_comment)]
	ignore_feature: Vec<String>,

	/// Also print the paths of the offending Cargo.toml files.
	#[clap(long)]
	print_paths: bool,
}

/// How to format the entries of a feature.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum Mode {
	/// Do nothing. This supersedes all other modes.
	None,
	/// Alphabetically sort the feature entries.
	Sort,
	/// Canonicalize the formatting of the feature entries.
	///
	/// This means that the order is not changed but that white spaces and newlines are normalized.
	/// Comments are kept but also normalized.
	Canonicalize,
}

impl FromStr for Mode {
	type Err = std::string::ParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_ascii_lowercase().as_str() {
			"canonicalize" => Ok(Self::Canonicalize),
			"sort" => Ok(Self::Sort),
			"none" => Ok(Self::None),
			_ => panic!("Invalid Mode: {s}. Expected 'canonicalize' or 'sort'"), // FIXME
		}
	}
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
		let modes = self.parse_mode_per_feature();
		let meta = self.cargo_args.load_metadata().expect("Loads metadata");
		// modifications are only allowed in this dir:
		let allowed_dir = canonicalize(&self.cargo_args.manifest_path).unwrap();
		let allowed_dir = allowed_dir.parent().unwrap();
		let mut offenders = Vec::new();

		log::info!("Checking {} crate{}", meta.packages.len(), plural(meta.packages.len()),);

		for pkg in meta.packages.iter() {
			let path = canonicalize(pkg.manifest_path.clone().into_std_path_buf()).unwrap();

			let mut fixer = AutoFixer::from_manifest(&path).unwrap();
			fixer.format_features(&modes).unwrap();
			if fixer.modified() {
				offenders.push((path, &pkg.name, fixer));
			}
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
			"Found {} crate{} with unformatted features:",
			global.red(&offenders.len().to_string()),
			plural(offenders.len())
		);
		for (path, pkg, fixer) in offenders.iter_mut() {
			// trim of the allowed_dir, if possible:
			let psuffix =
				self.print_paths.then(|| format!(" {}", path.display())).unwrap_or_default();
			println!("  {}{}", global.bold(pkg), psuffix);

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

			fixer.save().unwrap();
			fixed += 1;
		}

		if self.fix {
			println!("Formatted {} crate{}.", global.green(&fixed.to_string()), plural(fixed));

			if fixed != offenders.len() {
				log::error!(
					"ASSERT FAILED THIS IS A BUG: {} crate{} could not be formatted",
					offenders.len() - fixed,
					plural(offenders.len() - fixed)
				);
			}
			std::process::exit(0);
		} else {
			println!("Run again with --fix to format them.");
		}

		std::process::exit(1);
	}

	fn parse_mode_per_feature(&self) -> Map<String, Vec<Mode>> {
		let mut map = Map::<String, Vec<Mode>>::new();
		if let Some(modes) = &self.mode_per_feature {
			for (feature, mode) in modes.iter() {
				map.entry(feature.clone()).or_default().push(mode.clone());
			}
		}

		// Respect the ignore_feature flag:
		for feature in self.ignore_feature.iter() {
			map.insert(feature.clone(), vec![Mode::None]);
		}

		map
	}
}
