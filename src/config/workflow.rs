// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Loads config and workflow files.

use crate::{cmd::GlobalArgs, config::semver::Semver, log};
use serde::Deserialize;
use std::{collections::BTreeMap as Map, str::FromStr};

pub type WorkflowName = String;

/// The name of the workflow to run when none is specified.
pub const WORKFLOW_DEFAULT_NAME: &str = "default";

#[derive(Deserialize, Debug)]
pub struct WorkflowFile {
	version: Version,
	workflows: Map<WorkflowName, Workflow>,
	help: Option<WorkflowHelp>,
}

#[derive(Deserialize, Debug)]
pub struct Version {
	#[serde(deserialize_with = "Semver::from_serde")]
	format: Semver,

	#[serde(deserialize_with = "Semver::from_serde")]
	binary: Semver,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Workflow(pub Vec<WorkflowStep>);

#[derive(Deserialize, Clone, Debug)]
pub struct WorkflowStep(pub Vec<String>);

#[derive(Deserialize, Clone, Debug)]
pub struct WorkflowHelp {
	pub text: String,
	pub links: Vec<String>,
}

impl Workflow {
	pub fn run(self, _g: &GlobalArgs) -> Result<(), String> {
		for (_i, step) in self.0.iter().enumerate() {
			let mut args = step.0.clone();
			// No default hint since the workflows can provide their own.
			args.push("--fix-hint=off".into());
			let cmd = std::env::args().next().unwrap_or("zepter".into());

			log::debug!("Running command '{cmd} {}'", args.join(" "));

			let status = std::process::Command::new(&cmd)
				.args(args.clone())
				.status()
				.map_err(|e| format!("Failed to run command '{cmd}': {e}"))?;

			let first_two_args = args
				.iter()
				.rev()
				.skip(1)
				.rev()
				.take(2)
				.map(String::as_str)
				.collect::<Vec<_>>()
				.join(" ");

			if !status.success() {
				return Err(format!(
					"Command '{first_two_args}' failed with exit code {}",
					status.code().unwrap_or(1)
				))
			}

			log::info!("{}/{} {:<}", _i + 1, self.0.len(), first_two_args);
		}

		Ok(())
	}
}

impl FromStr for WorkflowFile {
	type Err = String;

	fn from_str(content: &str) -> Result<Self, Self::Err> {
		let parsed = serde_yaml_ng::from_str::<WorkflowFile>(content)
			.map_err(|e| format!("yaml parsing: {e}"))?;

		if parsed.version.format != (1, 0, 0).into() {
			return Err("Can only parse workflow files with version '1'".into())
		}

		parsed.into_resolved()
	}
}

impl WorkflowFile {
	pub fn workflow<S: AsRef<str>>(&self, name: S) -> Option<Workflow> {
		self.workflows.get(name.as_ref()).cloned()
	}

	/// Load a workflow file from the given path.
	pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> Result<Self, String> {
		let path = path.as_ref();
		let content = std::fs::read_to_string(path)
			.map_err(|e| format!("Failed to read config file {path:?}: {e}"))?;

		content.parse()
	}

	/// Format the user-provided help message.
	pub fn fmt_help(&self) -> Option<String> {
		let help = self.help.as_ref()?;

		let links = if !help.links.is_empty() {
			format!(
				"\n\nFor more information, see:\n{}",
				help.links.iter().map(|s| format!("  - {s}")).collect::<Vec<_>>().join("\n")
			)
		} else {
			Default::default()
		};

		let text = help.text.strip_suffix('\n').unwrap_or(&help.text);
		format!("{text}{links}").into()
	}

	/// Iteratively resolve all references in the workflow file.
	pub fn into_resolved(mut self) -> Result<Self, String> {
		while self.resolve_once()? {}
		Ok(self)
	}

	/// Do one iterative resolve step and return whether something changed.
	pub fn resolve_once(&mut self) -> Result<bool, String> {
		let wfs = self.workflows.clone();

		for wf in self.workflows.values_mut() {
			for step in wf.0.iter_mut() {
				for (i, orig_line) in step.0.iter_mut().enumerate() {
					if let Some(line) = orig_line.strip_prefix('$') {
						let (vname, index) = line.split_once('.')
						.ok_or_else(|| format!("Expected '$name.index' format, got '${line}'"))?;
						let index: u32 = index.parse().map_err(|e| {
							format!("Failed to parse index '{index}' in line '{line}': {e}")
						})?;

						let value = wfs.get(vname).ok_or_else(|| {
							format!("Failed to find workflow '{vname}' in line '{line}'")
						})?;

						let referenced_step = value.0.get(index as usize)
							.ok_or_else(|| format!("Index {index} out of bounds for workflow '{vname}' which has {} steps", value.0.len()))?;
						step.0.remove(i);
						for line in referenced_step.0.iter().rev() {
							step.0.insert(i, line.clone());
						}

						return Ok(true)
					}
				}
			}
		}

		Ok(false)
	}

	/// Whether the config file is compatible with the current version of the running binary.
	pub fn check_cfg_compatibility(&self) -> Result<(), String> {
		let current_version =
			Semver::try_from(clap::crate_version!()).expect("Crate version is valid semver");
		let required_version = self.version.binary;

		if current_version.is_newer_or_equal(&required_version) {
			Ok(())
		} else {
			Err(format!(
				"Your version of Zepter is too old for this project.\n\n Required:  {required_version}\n Installed: {current_version}\n\nPlease update Zepter with:\n\n  cargo install zepter --locked\n\nOr add `--check-cfg-compatibility=off` to the config file."
			))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn parse(yaml: &str) -> Result<WorkflowFile, String> {
		yaml.parse()
	}

	#[test]
	fn workflow_file_from_yaml_works() {
		let cfg = WorkflowFile::from_path("presets/polkadot.yaml").unwrap();
		assert_eq!(cfg.workflows.len(), 2);
		assert_eq!(cfg.workflow("check").unwrap().0.len(), 2);
		assert_eq!(cfg.workflow("default").unwrap().0.len(), 2);
	}

	#[test]
	fn from_path_missing_file() {
		let err = WorkflowFile::from_path("nonexistent.yaml").unwrap_err();
		assert!(err.contains("Failed to read config file"), "{err}");
	}

	#[test]
	fn rejects_unsupported_format_version() {
		let yaml = "
version:
  format: 2
  binary: 0.1.0
workflows: {}
";
		let err = parse(yaml).unwrap_err();
		assert!(err.contains("version '1'"), "{err}");
	}

	#[test]
	fn rejects_invalid_yaml() {
		let Err(err) = parse("not: valid: yaml: {{{}}}") else { panic!("expected error") };
		assert!(err.contains("yaml parsing"), "{err}");
	}

	#[test]
	fn workflow_lookup_missing_returns_none() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  check:
    - ['lint', 'propagate-feature']
";
		let cfg = parse(yaml).unwrap();
		assert!(cfg.workflow("nonexistent").is_none());
	}

	#[test]
	fn resolve_references() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  base:
    - ['lint', 'propagate-feature']
  derived:
    - [ $base.0, '--fix' ]
";
		let cfg = parse(yaml).unwrap();
		let derived = cfg.workflow("derived").unwrap();
		assert_eq!(derived.0.len(), 1);
		assert!(derived.0[0].0.contains(&"lint".to_string()));
		assert!(derived.0[0].0.contains(&"--fix".to_string()));
	}

	#[test]
	fn resolve_bad_index_errors() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  base:
    - ['lint']
  broken:
    - [ '$base.notanumber' ]
";
		let err = parse(yaml).unwrap_err();
		assert!(err.contains("Failed to parse index"), "{err}");
	}

	#[test]
	fn resolve_missing_workflow_errors() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  broken:
    - [ '$nonexistent.0' ]
";
		let err = parse(yaml).unwrap_err();
		assert!(err.contains("Failed to find workflow"), "{err}");
	}

	#[test]
	fn fmt_help_with_links() {
		let cfg = WorkflowFile::from_path("presets/polkadot.yaml").unwrap();
		let help = cfg.fmt_help().unwrap();
		assert!(help.contains("Polkadot-SDK"), "{help}");
		assert!(help.contains("For more information"), "{help}");
	}

	#[test]
	fn fmt_help_without_links() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows: {}
help:
  text: |
    some help text
  links: []
";
		let cfg = parse(yaml).unwrap();
		let help = cfg.fmt_help().unwrap();
		assert!(help.contains("some help text"), "{help}");
		assert!(!help.contains("For more information"), "{help}");
	}

	#[test]
	fn fmt_help_none() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows: {}
";
		let cfg = parse(yaml).unwrap();
		assert!(cfg.fmt_help().is_none());
	}

	#[test]
	fn fmt_help_preserves_text_without_trailing_newline() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows: {}
help:
  text: 'no trailing newline'
  links: []
";
		let cfg = parse(yaml).unwrap();
		let help = cfg.fmt_help().unwrap();
		assert!(help.contains("no trailing newline"), "{help}");
	}

	/// BUG: `$ref` without a `.index` should return Err, not panic.
	#[test]
	fn resolve_ref_without_dot_returns_error() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  base:
    - ['lint']
  broken:
    - [ '$base' ]
";
		let err = parse(yaml).unwrap_err();
		assert!(err.contains("base"), "{err}");
	}

	/// BUG: out-of-bounds index should return Err, not panic.
	#[test]
	fn resolve_out_of_bounds_index_returns_error() {
		let yaml = "
version:
  format: 1
  binary: 0.1.0
workflows:
  base:
    - ['lint']
  broken:
    - [ '$base.99' ]
";
		let err = parse(yaml).unwrap_err();
		assert!(err.contains("99"), "{err}");
	}

	#[test]
	fn check_cfg_compatibility_passes_for_current_version() {
		let yaml = format!(
			"
version:
  format: 1
  binary: {}
workflows: {{}}
",
			clap::crate_version!()
		);
		let cfg = parse(&yaml).unwrap();
		cfg.check_cfg_compatibility().unwrap();
	}

	#[test]
	fn check_cfg_compatibility_fails_for_future_version() {
		let yaml = "
version:
  format: 1
  binary: 255.0.0
workflows: {}
";
		let cfg = parse(yaml).unwrap();
		let err = cfg.check_cfg_compatibility().unwrap_err();
		assert!(err.contains("too old"), "{err}");
	}
}
