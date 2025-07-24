// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Helpers for writing tests.

#![cfg(feature = "testing")]

pub mod git;
pub use git::*;

use std::{
	collections::{BTreeMap, HashMap},
	fs,
	io::Write,
	path::{Path, PathBuf},
	process::Command,
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
		.map_err(|e| anyhow::anyhow!("Failed to write case file: {}", e))
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
		serde_yaml_ng::from_str(&content).map_err(|e| anyhow::anyhow!("Failed to parse: {}", e))
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
		serde_yaml_ng::from_str(&content).map_err(|e| anyhow::anyhow!("Failed to parse: {}", e))
	}

	pub fn init(&self) -> Result<PathBuf, anyhow::Error> {
		clone_repo(&self.repo.name, &self.repo.ref_spec)
	}
}

/// Describes a Rust crate, its features and dependencies.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CrateConfig {
	name: ModuleName,
	#[serde(skip_serializing_if = "Option::is_none")]
	deps: Option<Vec<CrateDependency>>,
	#[allow(clippy::type_complexity)]
	#[serde(skip_serializing_if = "Option::is_none")]
	features: Option<BTreeMap<String, Option<Vec<(String, String)>>>>,
}

impl CrateConfig {
	/// Return the file path of this crate.
	pub fn path(&self) -> String {
		crate_name_to_path(&self.name)
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

pub struct Context {
	pub root: tempfile::TempDir,
}

impl Default for Context {
	fn default() -> Self {
		Self::new()
	}
}

impl Context {
	pub fn new() -> Self {
		Self { root: tempfile::tempdir().expect("Must create a temporary directory") }
	}

	pub fn persist(self) -> PathBuf {
		self.root.keep()
	}

	pub fn create_crate(&self, module: &CrateConfig) -> Result<(), anyhow::Error> {
		self.cargo(
			&format!("new --vcs=none --offline --lib --name {} {}", module.name, module.path()),
			None,
		)?;
		let toml_path = self.root.path().join(module.path()).join("Cargo.toml");
		assert!(toml_path.exists(), "Crate must exist");
		// Add the deps
		let mut out_deps = HashMap::<cargo_metadata::DependencyKind, String>::new();
		for dep in module.deps.iter().flatten() {
			out_deps.entry(dep.kind()).or_default().push_str(&dep.def());
		}

		let mut txt = String::from("[features]\n");
		for (feature, enables) in module.features.iter().flatten() {
			txt.push_str(&format!("{feature} = [\n"));
			for (dep, feat) in enables.iter().flatten() {
				txt.push_str(&format!("\"{dep}/{feat}\",\n"));
			}
			txt.push_str("]\n");
		}

		let deps = format!(
			"{}\n[dev-dependencies]\n{}\n[build-dependencies]\n{}\n",
			out_deps.remove(&cargo_metadata::DependencyKind::Normal).unwrap_or_default(),
			out_deps
				.remove(&cargo_metadata::DependencyKind::Development)
				.unwrap_or_default(),
			out_deps.remove(&cargo_metadata::DependencyKind::Build).unwrap_or_default(),
		);

		let output = format!("{deps}\n{txt}");
		// Append to the toml
		let mut file = fs::OpenOptions::new().append(true).open(toml_path).unwrap();
		file.write_all(output.as_bytes()).unwrap();
		Ok(())
	}

	pub fn create_workspace(&self, subs: &[CrateConfig]) -> Result<(), anyhow::Error> {
		let mut txt = String::from("[workspace]\nmembers = [");
		for sub in subs.iter() {
			txt.push_str(&format!("\"{}\",", sub.path()));
		}
		txt.push(']');
		let toml_path = self.root.path().join("Cargo.toml");
		fs::write(toml_path, txt)?;
		Ok(())
	}

	pub fn cargo(&self, cmd: &str, sub_dir: Option<&str>) -> Result<(), anyhow::Error> {
		assert!(self.root.path().exists());
		let dir = match sub_dir {
			Some(sub_dir) => self.root.path().join(sub_dir),
			None => self.root.path().to_owned(),
		};

		let args = cmd.split_whitespace().collect::<Vec<_>>();
		let output = Command::new("cargo")
			.args(&args)
			.current_dir(&dir)
			.output()
			.expect("failed to execute cargo");

		if !output.status.success() {
			Err(anyhow::Error::msg(String::from_utf8(output.stderr).unwrap()))
		} else {
			Ok(())
		}
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

/// Describes a crate dependency.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(untagged)]
pub enum CrateDependency {
	Implicit(String),
	Explicit {
		name: String,
		#[serde(skip_serializing_if = "Option::is_none")]
		rename: Option<String>,
		#[serde(skip_serializing_if = "is_false")]
		optional: Option<bool>,
		#[serde(skip_serializing_if = "Option::is_none")]
		kind: Option<cargo_metadata::DependencyKind>,
	},
}

impl CrateDependency {
	fn def(&self) -> String {
		let option = if self.optional() { ", optional = true".to_string() } else { String::new() };
		let mut ret = if let Some(rename) = self.rename() {
			format!("{} = {{ package = \"{}\", ", rename, self.name())
		} else {
			format!("{} = {{ ", self.name())
		};
		ret.push_str(&format!("version = \"*\", path = \"../{}\"{}}}\n", self.path(), option));
		ret
	}

	fn path(&self) -> String {
		crate_name_to_path(&self.name())
	}

	fn kind(&self) -> cargo_metadata::DependencyKind {
		match self {
			Self::Explicit { kind, .. } => kind.unwrap_or_default(),
			Self::Implicit(_) => cargo_metadata::DependencyKind::Normal,
		}
	}

	fn name(&self) -> String {
		match self {
			Self::Explicit { name, .. } | Self::Implicit(name) => name.clone(),
		}
	}

	fn rename(&self) -> Option<String> {
		match self {
			Self::Explicit { rename, .. } => rename.clone(),
			Self::Implicit(_) => None,
		}
	}

	fn optional(&self) -> bool {
		match self {
			Self::Explicit { optional, .. } => optional.unwrap_or_default(),
			Self::Implicit(_) => false,
		}
	}
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

/// Predicate for serde to skip serialization of default values.
fn is_false(b: &Option<bool>) -> bool {
	!b.unwrap_or_default()
}
