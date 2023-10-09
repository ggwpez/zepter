// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Loads config and workflow files.

use crate::{cmd::GlobalArgs, config::semver::Semver, log};
use serde::Deserialize;
use std::collections::BTreeMap as Map;

pub type WorkflowName = String;

/// The name of the workflow to run when none is specified.
pub const WORKFLOW_DEFAULT_NAME: &str = "default";

#[derive(Deserialize)]
pub struct WorkflowFile {
	version: Version,
	workflows: Map<WorkflowName, Workflow>,
	help: Option<WorkflowHelp>,
}

#[derive(Deserialize)]
pub struct Version {
	#[serde(deserialize_with = "Semver::from_serde")]
	format: Semver,

	#[serde(deserialize_with = "Semver::from_serde")]
	binary: Semver,
}

#[derive(Deserialize, Clone)]
pub struct Workflow(pub Vec<WorkflowStep>);

#[derive(Deserialize, Clone)]
pub struct WorkflowStep(pub Vec<String>);

#[derive(Deserialize, Clone)]
pub struct WorkflowHelp {
	pub text: String,
	pub links: Vec<String>,
}

impl Workflow {
	pub fn run(self, _g: &GlobalArgs) -> Result<(), String> {
		for (i, step) in self.0.iter().enumerate() {
			let _ = i;
			let args = &step.0;
			let cmd = std::env::args().next().unwrap_or("zepter".into());

			log::debug!("Running command '{} {}'", cmd, args.join(" "));

			let status = std::process::Command::new(&cmd)
				.args(args.clone())
				.status()
				.map_err(|e| format!("Failed to run command '{}': {}", cmd, e))?;

			let first_two_args =
				args.iter().take(2).map(|s| s.as_str()).collect::<Vec<_>>().join(" ");

			if !status.success() {
				return Err(format!(
					"Command '{}' failed with exit code {}",
					first_two_args,
					status.code().unwrap_or(-1)
				))
			}

			log::info!("{}/{} {:<}", i + 1, self.0.len(), first_two_args);
		}

		Ok(())
	}
}

impl WorkflowFile {
	pub fn workflow<S: AsRef<str>>(&self, name: S) -> Option<Workflow> {
		self.workflows.get(name.as_ref()).cloned()
	}

	pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> Result<Self, String> {
		let path = path.as_ref();
		let content = std::fs::read_to_string(path)
			.map_err(|e| format!("Failed to read config file {:?}: {}", path, e))?;
		let parsed = serde_yaml::from_str::<WorkflowFile>(&content)
			.map_err(|e| format!("Failed to parse config file {:?}: {}", path, e))?;

		if parsed.version.format != (1, 0, 0).into() {
			return Err("Only format version '1' is currently supported.".into())
		}

		log::debug!("Workflows in config file: {:#?}", parsed.workflows.keys());

		parsed.into_resolved()
	}

	pub fn fmt_help(&self) -> Option<String> {
		let help = self.help.as_ref()?;

		let links = if !help.links.is_empty() {
			format!(
				"\n\nFor more information, see:\n{}",
				help.links.iter().map(|s| format!("  - {}", s)).collect::<Vec<_>>().join("\n")
			)
		} else {
			"".into()
		};

		let text = help.text.strip_suffix('\n').unwrap_or("");
		format!("{}{}", text, links).into()
	}

	pub fn into_resolved(mut self) -> Result<Self, String> {
		while self.resolve_once()? {}
		Ok(self)
	}

	pub fn resolve_once(&mut self) -> Result<bool, String> {
		let wfs = self.workflows.clone();

		for (_name, wf) in self.workflows.iter_mut() {
			for step in wf.0.iter_mut() {
				for (i, orig_line) in step.0.iter_mut().enumerate() {
					if let Some(line) = orig_line.strip_prefix('$') {
						let (vname, index) = line.split_once('.').expect("Expecting $name.index");
						let index: u32 = index.parse().map_err(|e| {
							format!("Failed to parse index '{}' in line '{}': {}", index, line, e)
						})?;

						let value = wfs.get(vname).ok_or_else(|| {
							format!("Failed to find workflow '{}' in line '{}'", vname, line)
						})?;

						step.0.remove(i);
						for line in value.0[index as usize].0.iter().rev() {
							step.0.insert(i, line.clone());
						}

						return Ok(true)
					}
				}
			}
		}

		Ok(false)
	}

	pub fn check_cfg_compatibility(&self) -> Result<(), String> {
		let current_version =
			Semver::try_from(clap::crate_version!()).expect("Crate version is valid semver");
		let file_version = self.version.binary;

		if current_version.is_newer_or_equal(&file_version) {
			Ok(())
		} else {
			Err(format!(
				"Config file version is too new. The file requires at least {}, but the current version is {}. Please update Zepter or ignore this check with `--check-cfg-compatibility=off`.",
				file_version, current_version
			))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn workflow_file_from_yaml_works() {
		let cfg = WorkflowFile::from_path("presets/polkadot.yaml").unwrap();
		// Sanity checky only
		assert_eq!(cfg.workflows.len(), 2);
		assert_eq!(cfg.workflow("check").unwrap().0.len(), 2);
		assert_eq!(cfg.workflow("default").unwrap().0.len(), 2);
	}
}
