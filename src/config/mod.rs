// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

pub mod semver;
pub mod workflow;

use crate::{config::workflow::WorkflowFile, log};

use std::{
	fs::canonicalize,
	path::{Path, PathBuf},
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
			let path = canonicalize(path).expect("Must canonicalize path");

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
		cmd.arg("locate-project").args(["--workspace", "--offline", "--locked"]);
		if let Some(path) = &self.manifest_path {
			cmd.arg("--manifest-path").arg(path);
		}
		let output = cmd.output().expect("Failed to run `cargo locate-project`");
		let path = output.stdout;
		let path =
			String::from_utf8(path).expect("Failed to parse output of `cargo locate-project`");
		let path: serde_json::Value = serde_json::from_str(&path).unwrap_or_else(|_| {
			panic!(
				"Failed to parse output of `cargo locate-project`: '{}'",
				String::from_utf8_lossy(&output.stderr)
			)
		});
		let path = path["root"].as_str().expect("Failed to parse output of `cargo locate-project`");
		let path = PathBuf::from(path);
		let root = path.parent().expect("Failed to get parent of workspace root");

		Ok(root.into())
	}
}
