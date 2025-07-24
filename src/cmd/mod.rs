// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Sub-command definition and implementation.

pub mod debug;
pub mod fmt;
pub mod lint;
pub mod run;
pub mod trace;
pub mod transpose;

use crate::{log, ErrToStr};

use cargo_metadata::{Dependency, Metadata, MetadataCommand, Package, Resolve};
use std::{fs::canonicalize, path::Path};

/// See out how Rust dependencies and features are enabled.
#[derive(Debug, clap::Parser)]
#[command(author, version, about, long_about = None)]
pub struct Command {
	#[clap(subcommand)]
	subcommand: Option<SubCommand>,

	#[clap(flatten)]
	global: GlobalArgs,
}

#[derive(Debug, clap::Parser)]
pub struct GlobalArgs {
	/// Only print errors. Supersedes `--log`.
	#[clap(long, short, global = true)]
	quiet: bool,

	/// Log level to use.
	#[cfg(feature = "logging")]
	#[clap(long = "log", global = true, default_value = "info", ignore_case = true)]
	level: ::log::LevelFilter,

	/// LOGGING IS DISABLED IN THIS BUILD; FLAG IGNORED.
	#[cfg(not(feature = "logging"))]
	#[clap(long = "log", global = true, default_value = "info", ignore_case = true)]
	level: String,

	/// Use ANSI terminal colors.
	#[clap(long, global = true, default_value_t = false)]
	color: bool,

	/// Try to exit with code zero if the intended check failed.
	///
	/// Will still return 1 in case of an actual error (eg. failed to find some file) or a panic
	/// (aka software bug).
	#[clap(long, global = true, verbatim_doc_comment)]
	exit_code_zero: bool,

	/// Dont print any hints on how to fix the error.
	///
	/// This is mostly used internally when dispatching, workflows since they come with their
	/// hints.
	#[clap(long, global = true, value_enum, verbatim_doc_comment, default_value_t = FixHint::On)]
	fix_hint: FixHint,
}

/// Sub-commands of the [Root](Command) command.
#[derive(Debug, clap::Subcommand)]
enum SubCommand {
	Trace(trace::TraceCmd),
	Lint(lint::LintCmd),
	#[clap(alias = "fmt", alias = "f")]
	Format(fmt::FormatCmd),
	Run(run::RunCmd),
	#[clap(hide = true)]
	Transpose(transpose::TransposeCmd),
	Debug(debug::DebugCmd),
}

/// A hint on how to fix the error.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum FixHint {
	/// Prints some hint that is (hopefully) helpful.
	On,
	/// Prints no hint at all.
	Off,
	// NOTE: We could have a `Custom` here one day
}

impl Command {
	pub fn run(&self) -> Result<(), String> {
		self.global.setup_logging();

		match self.subcommand.as_ref() {
			Some(SubCommand::Transpose(cmd)) => cmd.run(&self.global),

			Some(SubCommand::Trace(cmd)) => {
				cmd.run(&self.global);
				Ok(())
			},
			Some(SubCommand::Lint(cmd)) => cmd.run(&self.global),
			Some(SubCommand::Format(cmd)) => {
				cmd.run(&self.global);
				Ok(())
			},
			Some(SubCommand::Run(cmd)) => {
				cmd.run(&self.global);
				Ok(())
			},
			Some(SubCommand::Debug(cmd)) => {
				cmd.run(&self.global);
				Ok(())
			},
			None => {
				run::RunCmd::default().run(&self.global);
				Ok(())
			},
		}
	}
}

impl GlobalArgs {
	pub fn setup_logging(&self) {
		#[cfg(feature = "logging")]
		if self.quiet {
			::log::set_max_level(::log::LevelFilter::Error);
		} else {
			::log::set_max_level(self.level);
		}
	}

	pub fn warn_unstable(&self) {
		log::warn!("Unstable feature - do not rely on this!");
	}
	pub fn show_hints(&self) -> bool {
		// We use exahausive match to make sure to not forget a new variant in the future.
		match self.fix_hint {
			FixHint::On => true,
			FixHint::Off => false,
		}
	}

	pub fn error_code(&self) -> i32 {
		if self.exit_code_zero {
			0
		} else {
			1
		}
	}

	pub fn red(&self, s: &str) -> String {
		if !self.color {
			s.to_string()
		} else {
			format!("\x1b[31m{s}\x1b[0m")
		}
	}

	pub fn yellow(&self, s: &str) -> String {
		if !self.color {
			s.to_string()
		} else {
			format!("\x1b[33m{s}\x1b[0m")
		}
	}

	pub fn green(&self, s: &str) -> String {
		if !self.color {
			s.to_string()
		} else {
			format!("\x1b[32m{s}\x1b[0m")
		}
	}

	pub fn bold(&self, s: &str) -> String {
		if !self.color {
			s.to_string()
		} else {
			format!("\x1b[1m{s}\x1b[0m")
		}
	}
}

/// Arguments for how to load cargo metadata from a workspace.
#[derive(Debug, Clone, clap::Parser, PartialEq)]
pub struct CargoArgs {
	/// Cargo manifest path or directory.
	///
	/// For directories it appends a `Cargo.toml`.
	#[arg(long, global = true)]
	pub manifest_path: Option<std::path::PathBuf>,

	/// Whether to only consider workspace crates.
	#[clap(long, global = true)]
	pub workspace: bool,

	/// Whether to use offline mode.
	#[clap(long, global = true)]
	pub offline: bool,

	/// Whether to use all the locked dependencies from the `Cargo.lock`.
	///
	/// Otherwise it may update some dependencies. For CI usage its a good idea to use it.
	#[clap(long, global = true)]
	pub locked: bool,

	#[clap(long, global = true)]
	pub all_features: bool,
}

impl CargoArgs {
	pub fn with_workspace(mut self, workspace: bool) -> Self {
		self.workspace = workspace;
		self
	}

	/// Load the metadata of the rust project.
	pub fn load_metadata(&self) -> Result<Metadata, String> {
		let err = match self.load_metadata_unsorted() {
			Ok(meta) => return Ok(meta),
			Err(err) => err,
		};

		if check_for_locked_error(&err) {
			Err("\nThe Cargo.lock file needs to be updated first since --locked is present.\n"
				.to_string())
		} else {
			Err(err)
		}
	}

	fn load_metadata_unsorted(&self) -> Result<Metadata, String> {
		let mut cmd = MetadataCommand::new();

		if let Some(ref manifest_path) = self.manifest_path {
			let manifest_path = if manifest_path.is_dir() {
				manifest_path.join("Cargo.toml")
			} else {
				manifest_path.clone()
			};
			cmd.manifest_path(&manifest_path);
		}

		cmd.features(cargo_metadata::CargoOpt::AllFeatures);

		if self.workspace {
			cmd.no_deps();
		}
		if self.offline {
			cmd.other_options(vec!["--offline".to_string()]);
		}
		if self.locked {
			cmd.other_options(vec!["--locked".to_string()]);
		}

		cmd.exec().map_err(|e| format!("Failed to load metadata: {e}"))
	}
}

fn check_for_locked_error(err: &str) -> bool {
	err.contains("needs to be updated but --locked was passed to prevent this")
}

/// Resolve the dependency `dep` of `pkg` within the metadata.
///
/// This checks whether the dependency is a workspace or external crate and resolves it accordingly.
pub(crate) fn resolve_dep(
	pkg: &Package,
	dep: &Dependency,
	meta: &Metadata,
) -> Option<RenamedPackage> {
	match meta.resolve.as_ref() {
		Some(resolve) => resolve_dep_from_graph(pkg, dep, (meta, resolve)),
		None => resolve_dep_from_workspace(dep, meta),
	}
}

/// Resolve the dependency `dep` within the workspace.
///
/// Errors if `dep` is not a workspace member.
pub(crate) fn resolve_dep_from_workspace(
	dep: &Dependency,
	meta: &Metadata,
) -> Option<RenamedPackage> {
	for work in meta.workspace_packages() {
		if work.name.to_string() == dep.name {
			let pkg = meta.packages.iter().find(|pkg| pkg.id == work.id).cloned();
			return pkg.map(|pkg| RenamedPackage::new(pkg, dep.rename.clone(), dep.optional))
		}
	}
	None
}

/// Resolve the dependency `dep` of `pkg` within the resolve graph.
///
/// The resolve graph should only be used for external crates. I did not try what happens for
/// workspace members - better don't do it.
pub(crate) fn resolve_dep_from_graph(
	pkg: &Package,
	dep: &Dependency,
	(meta, resolve): (&Metadata, &Resolve),
) -> Option<RenamedPackage> {
	let dep_name = dep.rename.clone().unwrap_or(dep.name.clone()).replace('-', "_");
	let resolved_pkg = resolve.nodes.iter().find(|node| node.id == pkg.id)?;
	let resolved_dep_id =
		resolved_pkg.deps.iter().find(|node| node.name.to_string() == dep_name)?;
	let resolve_dep = meta.packages.iter().find(|pkg| pkg.id == resolved_dep_id.pkg)?;

	Some(RenamedPackage::new(resolve_dep.clone(), dep.rename.clone(), dep.optional))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenamedPackage {
	pub pkg: Package,
	pub rename: Option<String>,
	pub optional: bool,
}

impl RenamedPackage {
	pub fn new(pkg: Package, rename: Option<String>, optional: bool) -> Self {
		Self { pkg, rename, optional }
	}

	pub fn name(&self) -> String {
		self.rename.clone().unwrap_or(self.pkg.name.to_string())
	}

	pub fn original_name(&self) -> String {
		self.pkg.name.to_string()
	}

	pub fn display_name(&self) -> String {
		match &self.rename {
			Some(rename) => format!("{} (renamed from {})", rename, self.pkg.name),
			None => self.pkg.name.to_string(),
		}
	}
}

impl Ord for RenamedPackage {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		// Yikes... dafuq is this?!
		//bincode::serialize(self).unwrap().cmp(&bincode::serialize(other).unwrap())

		(&self.pkg.name, &self.pkg.id).cmp(&(&other.pkg.name, &other.pkg.id))
	}
}

impl PartialOrd for RenamedPackage {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

/// Parse a single key-value pair
///
/// Copy & paste from <https://github.com/clap-rs/clap/blob/master/examples/typed-derive.rs>
pub(crate) fn parse_key_val<T, U>(
	s: &str,
) -> Result<(T, U), Box<dyn std::error::Error + Send + Sync + 'static>>
where
	T: std::str::FromStr,
	T::Err: std::error::Error + Send + Sync + 'static,
	U: std::str::FromStr,
	U::Err: std::error::Error + Send + Sync + 'static,
{
	let s = s.trim_matches('"');
	let pos = s.find(':').ok_or_else(|| format!("invalid KEY=value: no `:` found in `{s}`"))?;
	Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

pub(crate) fn check_can_modify<P: AsRef<Path>>(root: P, modify: P) -> Result<bool, String> {
	let root = canonicalize(root).err_to_str()?;
	let modify = canonicalize(modify).err_to_str()?;

	if !modify.starts_with(&root) {
		println!(
			"Path is outside of the workspace: {:?} (not in {:?})",
			modify.display(),
			root.display()
		);
		Ok(false)
	} else {
		Ok(true)
	}
}
