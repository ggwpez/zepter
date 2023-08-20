// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Sub-command definition and implementation.

pub mod fmt;
pub mod lint;
pub mod trace;

use crate::log;
use cargo_metadata::{Dependency, Metadata, MetadataCommand, Package, Resolve};

/// See out how Rust dependencies and features are enabled.
#[derive(Debug, clap::Parser)]
#[command(author, version, about, long_about = None)]
pub struct Command {
	#[clap(subcommand)]
	subcommand: SubCommand,

	#[clap(flatten)]
	global: GlobalArgs,
}

#[derive(Debug, clap::Parser)]
pub struct GlobalArgs {
	/// Only print errors. Supersedes `--log`.
	#[clap(long, global = true)]
	quiet: bool,

	/// Log level to use.
	#[cfg(feature = "logging")]
	#[clap(long = "log", global = true, default_value = "info", ignore_case = true)]
	level: ::log::Level,

	/// Log level to use.
	#[cfg(not(feature = "logging"))]
	#[clap(long = "log", global = true, default_value = "info", ignore_case = true)]
	level: String,

	/// Use ANSI terminal colors.
	#[clap(long, global = true, default_value_t = false)]
	color: bool,
}

/// Sub-commands of the [Root](Command) command.
#[derive(Debug, clap::Subcommand)]
enum SubCommand {
	Trace(trace::TraceCmd),
	Lint(lint::LintCmd),
	Format(fmt::FormatCmd),
}

impl Command {
	pub fn run(&self) {
		self.global.setup_logging();

		match &self.subcommand {
			SubCommand::Trace(cmd) => cmd.run(&self.global),
			SubCommand::Lint(cmd) => cmd.run(&self.global),
			SubCommand::Format(cmd) => cmd.run(&self.global),
		}
	}
}

impl GlobalArgs {
	pub fn setup_logging(&self) {
		#[cfg(feature = "logging")]
		if self.quiet {
			::log::set_max_level(::log::LevelFilter::Error);
		} else {
			::log::set_max_level(self.level.to_level_filter());
		}
	}

	pub fn red(&self, s: &str) -> String {
		if !self.color {
			s.to_string()
		} else {
			format!("\x1b[31m{}\x1b[0m", s)
		}
	}

	pub fn yellow(&self, s: &str) -> String {
		if !self.color {
			s.to_string()
		} else {
			format!("\x1b[33m{}\x1b[0m", s)
		}
	}

	pub fn green(&self, s: &str) -> String {
		if !self.color {
			s.to_string()
		} else {
			format!("\x1b[32m{}\x1b[0m", s)
		}
	}

	pub fn bold(&self, s: &str) -> String {
		if !self.color {
			s.to_string()
		} else {
			format!("\x1b[1m{}\x1b[0m", s)
		}
	}
}

/// Arguments for how to load cargo metadata from a workspace.
#[derive(Debug, clap::Parser)]
pub struct CargoArgs {
	/// Cargo manifest path or directory.
	///
	/// For directories it appends a `Cargo.toml`.
	#[arg(long, global = true, default_value = "Cargo.toml")]
	pub manifest_path: std::path::PathBuf,

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
	/// Load the metadata of the rust project.
	pub fn load_metadata(&self) -> Result<Metadata, String> {
		let mut cmd = MetadataCommand::new();
		let manifest_path = if self.manifest_path.is_dir() {
			self.manifest_path.join("Cargo.toml")
		} else {
			self.manifest_path.clone()
		};
		log::debug!("Manifest at: {:?}", manifest_path);
		cmd.manifest_path(&manifest_path);
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
		if work.name == dep.name {
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
	let resolved_dep_id = resolved_pkg.deps.iter().find(|node| node.name == dep_name)?;
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
		self.rename.clone().unwrap_or(self.pkg.name.clone())
	}

	pub fn display_name(&self) -> String {
		match &self.rename {
			Some(rename) => format!("{} (renamed from {})", rename, self.pkg.name),
			None => self.pkg.name.clone(),
		}
	}
}

impl Ord for RenamedPackage {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		// Yikes... dafuq is this?!
		//bincode::serialize(self).unwrap().cmp(&bincode::serialize(other).unwrap())

		self.pkg.id.cmp(&other.pkg.id)
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
