// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Format features in the crate manifest.

use super::GlobalArgs;
use crate::{autofix::*, cmd::parse_key_val, grammar::*, log};

use cargo_metadata::Metadata;
use std::{collections::BTreeMap as Map, fs::canonicalize, path::PathBuf, str::FromStr};

/// Format the features in your manifest files.
#[derive(Debug, clap::Parser)]
pub struct FormatCmd {
	#[clap(subcommand)]
	subcommand: SubCommand,
}

/// Sub-commands of the [Format](FormatCmd) command.
#[derive(Debug, clap::Subcommand)]
pub enum SubCommand {
	#[clap(alias = "f")]
	Features(FormatFeaturesCmd),
}

/// Format the content of each feature in the crate manifest.
#[derive(Debug, clap::Parser)]
pub struct FormatFeaturesCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: super::CargoArgs,

	/// Include dependencies in the formatting check.
	///
	/// They will not be modified, unless their path is included in `--modify-paths`.
	#[clap(long)]
	no_workspace: bool,

	/// Paths that are allowed to be modified by the formatter.
	#[clap(long)]
	modify_paths: Vec<PathBuf>,

	/// DEPRECATED AND IGNORED
	#[clap(long = "check", short = 'c')]
	unused_check: bool,

	/// Fix the formatting errors automatically.
	#[clap(long, short)]
	fix: bool,

	/// The maximal length of a line for a feature.
	#[clap(long, default_value_t = 80)]
	line_width: u32,

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
	Dedub,
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
			_ => panic!("Invalid Mode: {s}. Expected 'canonicalize', 'sort' or 'none'"), // FIXME
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
		if self.unused_check {
			log::warn!("The `--check` is now implicit and ignored");
		}

		let modes = self.parse_mode_per_feature();
		let meta = self.load_metadata(global);
		// Allowed dir that we can write to.
		let allowed_dir = canonicalize(meta.workspace_root.as_std_path()).unwrap();
		log::debug!("Allowed dir: {}", allowed_dir.display());
		let mut offenders = Vec::new();
		// (path, crate) -> errors
		let mut errors = Map::<(PathBuf, String), Vec<String>>::new();

		log::debug!("Checking {} crate{}", meta.packages.len(), plural(meta.packages.len()));

		for pkg in meta.packages.iter() {
			let path = canonicalize(pkg.manifest_path.clone().into_std_path_buf()).unwrap();

			let mut fixer = AutoFixer::from_manifest(&path).unwrap();
			if let Err(errs) = fixer.canonicalize_features(&pkg.name, &modes, self.line_width) {
				let path = path.strip_prefix(&allowed_dir).unwrap().to_path_buf();
				errors.entry((path.clone(), pkg.name.clone())).or_default().extend(errs);
			} else if fixer.modified() {
				offenders.push((path, &pkg.name, fixer));
			}
		}
		if !errors.is_empty() {
			let num_errors = errors.values().map(|errs| errs.len()).sum::<usize>();
			println!(
				"Please fix {} error{} in {} crate{} manually:",
				global.red(&num_errors.to_string()),
				plural(num_errors),
				global.red(&errors.len().to_string()),
				plural(errors.len())
			);
			for ((path, pkg), errs) in errors.iter() {
				println!("  {} ({})", global.bold(pkg), path.display());
				for err in errs.iter() {
					println!("    {err}");
				}
			}
			std::process::exit(global.error_code())
		}

		if offenders.is_empty() {
			log::debug!(
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

			let can_modify = path.starts_with(&allowed_dir) ||
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
			if fixed == offenders.len() {
				println!(
					"Formatted {} crate{} (all fixed).",
					global.green(&fixed.to_string()),
					plural(fixed)
				);
			} else {
				println!(
					"Formatted {} crate{} ({} could not be fixed).",
					global.green(&fixed.to_string()),
					plural(fixed),
					global.red(&(offenders.len() - fixed).to_string())
				);
			}

			std::process::exit(0);
		} else if global.show_hints() {
			println!("Run again with `--fix` to format them.");
		}

		std::process::exit(global.error_code())
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

	fn load_metadata(&self, global: &GlobalArgs) -> Metadata {
		let mut args = self.cargo_args.clone();
		if args.workspace {
			println!("{}", global.yellow("WARNING: --workspace is the default now"));
		}
		args.workspace = !self.no_workspace;
		match args.load_metadata() {
			Ok(meta) => meta,
			Err(err) => {
				println!("{}", global.red(&err));
				std::process::exit(1)
			},
		}
	}
}
