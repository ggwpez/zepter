// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

pub mod semver;
pub mod workflow;

use crate::{cmd::GlobalArgs, config::workflow::WorkflowFile, log, ErrToStr};

use std::{
	fs::canonicalize,
	path::{absolute, Path, PathBuf},
};

#[derive(Default, Debug, clap::Parser)]
pub struct ConfigArgs {
	/// Manually set the location of the manifest file.
	///
	/// Must point directly to a file an not a directory.
	#[clap(long, global = true)]
	pub manifest_path: Option<std::path::PathBuf>,

	/// The path to the config file to use.
	#[clap(long, alias = "cfg", short)]
	pub config: Option<std::path::PathBuf>,

	/// Whether to check if the config file is compatible with the current version of Zepter.
	#[clap(long, value_enum, value_name = "TOGGLE", default_value_t = Toggle::On, verbatim_doc_comment)]
	pub check_cfg_compatibility: Toggle,
}

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum Toggle {
	On,
	Off,
}

impl Default for Toggle {
	fn default() -> Self {
		Self::On
	}
}

pub const WELL_KNOWN_CFG_PATHS: &[&str] = &["zepter.yaml", ".zepter.yaml"];

/// Search for `zepter.yaml`, `zepter`, `.zepter.yaml` or `.zepter` in the folders:
/// - `./`
/// - `./.cargo/`
/// - `./.config`
pub fn search_config<P: AsRef<Path>>(workspace: P) -> Result<PathBuf, Vec<PathBuf>> {
	let paths: Vec<PathBuf> = vec![
		workspace.as_ref().to_path_buf(),
		workspace.as_ref().join(".cargo"),
		workspace.as_ref().join(".config"),
	];
	let mut searched = vec![];

	// Check all combinations of names and paths
	for (name, path) in WELL_KNOWN_CFG_PATHS
		.iter()
		.flat_map(|name| paths.iter().map(move |path| (name, path)))
	{
		let mut path = path.join(name);

		if path.exists() {
			path = canonicalize(path).expect("Failed to canonicalize path");
			return Ok(path)
		}
		searched.push(path);
	}

	Err(searched)
}

impl ConfigArgs {
	pub fn load_or_panic(&self) -> WorkflowFile {
		self.load().unwrap_or_else(|e| {
			eprintln!("{e}");
			std::process::exit(GlobalArgs::error_code_cfg_parsing());
		})
	}

	pub fn load(&self) -> Result<WorkflowFile, String> {
		let path = self.locate_config()?;
		log::debug!("Using config file: {path:?}");
		let cfg = WorkflowFile::from_path(path)?;

		if self.check_cfg_compatibility == Toggle::On {
			cfg.check_cfg_compatibility()?;
		}

		Ok(cfg)
	}

	fn locate_config(&self) -> Result<PathBuf, String> {
		if let Some(path) = &self.config {
			let path = absolute(path).err_to_str()?;

			if path.exists() {
				Ok(path)
			} else {
				Err(format!("Provided config path does not exist: {path:?}"))
			}
		} else {
			let root = self.locate_workspace()?;

			match search_config(root) {
				Ok(cfg) => Ok(cfg),
				Err(searched) => {
					println!("Failed to find config file in any of these locations:");
					for path in searched {
						println!(" - {}", path.display());
					}
					Err("Could not find a config file".into())
				},
			}
		}
	}

	fn locate_workspace(&self) -> Result<PathBuf, String> {
		let mut cmd = std::process::Command::new("cargo");
		cmd.arg("locate-project").args([
			"--message-format",
			"plain",
			"--workspace",
			"--offline",
			"--locked",
		]);
		if let Some(path) = &self.manifest_path {
			cmd.arg("--manifest-path").arg(path);
		}
		let output = cmd.output().err_to_str()?;

		if !output.status.success() {
			let err = String::from_utf8(output.stderr).err_to_str()?;
			let err = err.replace("\n", "\n\t");
			return Err(format!(
				"Failed to find the workspace root with `cargo locate-project`:\n\n\t{err}"
			));
		}

		// `cargo locate-project` outputs a trailing newline that must be stripped.
		let path =
			String::from_utf8(output.stdout).map(|s| PathBuf::from(s.trim())).err_to_str()?;

		path.parent()
			.map(Into::into)
			.ok_or_else(|| format!("Failed to get parent directory of: {}", path.display()))
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn locate_config_nonexistent_path_gives_error() {
		let args = ConfigArgs {
			config: Some(PathBuf::from("/nonexistent/path/zepter.yaml")),
			..Default::default()
		};

		let err = args.load().unwrap_err();
		assert!(
			err.contains("does not exist"),
			"Expected 'does not exist' in error message, got: {err}"
		);
	}

	#[test]
	fn cargo_locate_project_has_trailing_newline() {
		let output = std::process::Command::new("cargo")
			.args([
				"locate-project",
				"--message-format",
				"plain",
				"--workspace",
				"--offline",
				"--locked",
			])
			.output()
			.expect("Failed to run cargo locate-project");

		let stdout = String::from_utf8(output.stdout).unwrap();
		assert!(
			stdout.ends_with('\n'),
			"Expected trailing newline in cargo locate-project output, got: {stdout:?}"
		);
	}
}
