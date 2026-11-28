// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Helpers for writing tests.

#![cfg(feature = "testing")]

pub mod git;
mod workspace;

pub use git::*;
pub use workspace::{Context, CrateConfig, CrateDependency};

use std::{
	fs,
	path::{Path, PathBuf},
};

pub type ModuleName = String;

/// A single test case.
///
/// Holds the input arguments, the stdout, an optional git diff and the exit code of the binary.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Case {
	pub cmd: String,

	#[serde(skip_serializing_if = "String::is_empty")]
	#[serde(default)]
	pub stdout: String,

	#[serde(skip_serializing_if = "String::is_empty")]
	#[serde(default)]
	pub stderr: String,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<i32>,

	#[serde(skip_serializing_if = "String::is_empty")]
	#[serde(default)]
	pub diff: String,

	#[serde(skip_serializing_if = "Option::is_none")]
	#[serde(default)]
	pub config: Option<ZepterConfig>,
}

/// A specific github repo checkout.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Repo {
	pub name: String,
	#[serde(rename = "ref")]
	pub ref_spec: String,
}

pub enum CaseFile {
	Ui(UiCaseFile),
	Integration(IntegrationCaseFile),
}

#[derive(Default)]
pub struct CaseCleanupGuard {
	cfg_path: Option<PathBuf>,
}

impl Case {
	pub fn init(&self, root: &Path) -> Result<CaseCleanupGuard, anyhow::Error> {
		let Some(cfg) = &self.config else { return Ok(CaseCleanupGuard::default()) };

		let cfg_path = cfg.write(root)?;
		Ok(CaseCleanupGuard { cfg_path: Some(cfg_path) })
	}
}

impl Drop for CaseCleanupGuard {
	fn drop(&mut self) {
		if let Some(p) = self.cfg_path.take() {
			fs::remove_file(p).unwrap();
		}
	}
}

impl CaseFile {
	pub fn from_file(path: &Path) -> Self {
		UiCaseFile::from_file(path)
			.map(CaseFile::Ui)
			.or_else(|_| IntegrationCaseFile::from_file(path).map(CaseFile::Integration))
			.unwrap_or_else(|e| panic!("Failed to parse file {path:?}: {e}"))
	}

	pub fn to_file(&self, path: &Path) -> Result<(), anyhow::Error> {
		let mut fd = fs::File::create(path).unwrap();

		match self {
			CaseFile::Ui(ui) => serde_yaml_ng::to_writer(&mut fd, &ui),
			CaseFile::Integration(integration) => serde_yaml_ng::to_writer(&mut fd, &integration),
		}
		.map_err(|e| anyhow::anyhow!("Failed to write case file: {e}"))
	}

	pub fn default_args(&self) -> bool {
		match self {
			CaseFile::Ui(ui) => !ui.no_default_args.unwrap_or_default(),
			CaseFile::Integration(ig) => !ig.no_default_args.unwrap_or_default(),
		}
	}

	pub fn cases(&self) -> &[Case] {
		match self {
			CaseFile::Ui(ui) => &ui.cases,
			CaseFile::Integration(integration) => &integration.cases,
		}
	}

	pub fn case_mut(&mut self, i: usize) -> &mut Case {
		match self {
			CaseFile::Ui(ui) => &mut ui.cases[i],
			CaseFile::Integration(integration) => &mut integration.cases[i],
		}
	}

	pub fn init(&self) -> Result<(PathBuf, Option<Context>), anyhow::Error> {
		match self {
			CaseFile::Ui(ui) => {
				let ctx = ui.init()?;
				Ok((ctx.root.path().to_owned(), Some(ctx)))
			},
			CaseFile::Integration(integration) => Ok((integration.init()?, None)),
		}
	}
}

/// Describes the setup for a UI test.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct UiCaseFile {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub comment: Option<String>,
	pub crates: Vec<CrateConfig>,
	pub cases: Vec<Case>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub configs: Option<Vec<ZepterConfig>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub no_default_args: Option<bool>,
}

impl UiCaseFile {
	pub fn init(&self) -> Result<Context, anyhow::Error> {
		let ctx = Context::new();
		for module in self.crates.iter() {
			ctx.create_crate(module)?;
		}
		ctx.create_workspace(&self.crates)?;
		git_init(ctx.root.path())?;
		self.generate_config(ctx.root.path())?;
		Ok(ctx)
	}

	pub fn from_file(path: &Path) -> Result<Self, anyhow::Error> {
		let content = fs::read_to_string(path)?;
		let content = content.replace('\t', "  ");
		serde_yaml_ng::from_str(&content).map_err(|e| anyhow::anyhow!("Failed to parse: {e}"))
	}

	fn generate_config(&self, root: &Path) -> Result<(), anyhow::Error> {
		let Some(configs) = &self.configs else { return Ok(()) };

		for cfg in configs.iter() {
			cfg.write(root)?;
		}

		Ok(())
	}
}

/// Describes the setup for an integration test.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct IntegrationCaseFile {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub comment: Option<String>,
	pub repo: Repo,
	pub cases: Vec<Case>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub no_default_args: Option<bool>,
}

impl IntegrationCaseFile {
	pub fn from_file(path: &Path) -> Result<Self, anyhow::Error> {
		let content = fs::read_to_string(path)?;
		let content = content.replace('\t', "  ");
		serde_yaml_ng::from_str(&content).map_err(|e| anyhow::anyhow!("Failed to parse: {e}"))
	}

	pub fn init(&self) -> Result<PathBuf, anyhow::Error> {
		clone_repo(&self.repo.name, &self.repo.ref_spec)
	}
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ZepterConfig {
	to_path: String,
	from_path: Option<String>,
	verbatim: Option<String>,
}

impl ZepterConfig {
	pub fn write(&self, root: &Path) -> Result<PathBuf, anyhow::Error> {
		let to_path = root.join(&self.to_path);
		fs::create_dir_all(to_path.parent().unwrap())?;

		assert!(
			self.verbatim.is_some() ^ self.from_path.is_some(),
			"Either `verbatim` or `from_path` must be set, but not both"
		);
		if let Some(verbatim) = &self.verbatim {
			fs::write(&to_path, verbatim)?;
		} else if let Some(from_path) = &self.from_path {
			let from_path = root.join(from_path);
			fs::copy(from_path, &to_path)?;
		}

		Ok(to_path)
	}
}

/// Convert a crate's name to a file path.
///
/// This is needed for case-insensitive file systems like on `MacOS`. It prefixes all lower-case
/// letters with an `l` and turns the upper case.
pub(crate) fn crate_name_to_path(n: &str) -> String {
	n.chars()
		.map(|c| if c.is_lowercase() { format!("l{}", c.to_uppercase()) } else { c.into() })
		.collect()
}

/// Removes leading and trailing empty lines.
pub fn normalize(s: &str) -> String {
	/*let mut lines = s.lines().collect::<Vec<_>>();
	while lines.first().map(|l| l.is_empty()).is_some() {
		lines.remove(0);
	}
	while lines.last().map(|l| l.is_empty()).is_some() {
		lines.pop();
	}
	format!("{}\n", lines.join("\n"))*/
	s.trim().to_string()
}
