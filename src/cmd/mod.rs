// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Sub-command definition and implementation.

pub mod debug;
pub mod fmt;
pub mod lint;
mod metadata;
pub mod run;
pub mod trace;
pub mod transpose;

pub(crate) use metadata::resolve_dep;
pub use metadata::{CargoArgs, RenamedPackage};

use crate::{log, ErrToStr};

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
	/// Will still return != 0 in case of an actual error (eg. failed to find some file) or a panic
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

/// A command failure and the status the CLI should return for it.
#[derive(Debug)]
pub enum CommandError {
	Failure(String),
	InvalidConfig(String),
	/// The command already printed the diagnostic, including any fix hints.
	Reported(u8),
}

impl CommandError {
	pub(crate) fn reported(code: u8) -> Self {
		Self::Reported(code)
	}
}

impl std::fmt::Display for CommandError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Failure(message) | Self::InvalidConfig(message) => f.write_str(message),
			Self::Reported(code) => write!(f, "command reported a failure (exit code {code})"),
		}
	}
}

impl std::error::Error for CommandError {}

impl From<String> for CommandError {
	fn from(message: String) -> Self {
		Self::Failure(message)
	}
}

impl From<&str> for CommandError {
	fn from(message: &str) -> Self {
		Self::Failure(message.to_owned())
	}
}

impl Command {
	pub fn run(&self) -> Result<(), CommandError> {
		self.global.setup_logging();

		match self.subcommand.as_ref() {
			Some(SubCommand::Transpose(cmd)) => cmd.run(&self.global)?,
			Some(SubCommand::Trace(cmd)) => cmd.run(&self.global)?,
			Some(SubCommand::Lint(cmd)) => cmd.run(&self.global)?,
			Some(SubCommand::Format(cmd)) => cmd.run(&self.global)?,
			Some(SubCommand::Run(cmd)) => cmd.run(&self.global)?,
			Some(SubCommand::Debug(cmd)) => cmd.run(&self.global)?,
			None => run::RunCmd::default().run(&self.global)?,
		}
		Ok(())
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

	pub fn error_code(&self) -> u8 {
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
