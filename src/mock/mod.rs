// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Helpers for writing tests.

#![cfg(feature = "testing")]

pub mod git;
pub use git::*;

use std::{path::Path, process::Command};
use assert_cmd::{assert::OutputAssertExt};
use std::{
	collections::HashMap,
	fs,
	path::{PathBuf},
};
use std::io::Write;

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

	#[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<i32>,

	#[serde(skip_serializing_if = "String::is_empty")]
	#[serde(default)]
	pub diff: String,
}

/// A specific github repo checkout.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Repo {
	pub name: String,
	#[serde(rename = "ref")]
	pub ref_spec: String,
}

/// Describes the setup for a UI test.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct UiCaseFile {
	pub crates: Vec<CrateConfig>,
	pub cases: Vec<Case>,
}

impl UiCaseFile {
	pub fn init(&self) -> Result<Context, anyhow::Error> {
		let ctx = Context::new();
		for module in self.crates.iter() {
			ctx.create_crate(module)?;
		}
		ctx.create_workspace(&self.crates)?;
		git_init(ctx.root.path())?;
		Ok(ctx)
	}

	pub fn from_file(path: &Path) -> Self {
		let content = fs::read_to_string(path).unwrap();
		let content = content.replace('\t', "  ");
		serde_yaml::from_str(&content)
			.unwrap_or_else(|_| panic!("Failed to parse: {}", &path.display()))
	}
}

/// Describes the setup for an integration test.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct IntegrationCaseFile {
	pub repo: Repo,
	pub cases: Vec<Case>,
}

impl IntegrationCaseFile {
	pub fn from_file(path: &Path) -> Self {
		let content = fs::read_to_string(path).unwrap();
		let content = content.replace('\t', "  ");
		serde_yaml::from_str(&content)
			.unwrap_or_else(|_| panic!("Failed to parse: {}", &path.display()))
	}

	pub fn init(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
		clone_repo(&self.repo.name, &self.repo.ref_spec)
	}
}

/// Describes a Rust crate, its features and dependencies.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CrateConfig {
	name: ModuleName,
	#[serde(skip_serializing_if = "Option::is_none")]
	deps: Option<Vec<CrateDependency>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	features: Option<HashMap<String, Option<Vec<(String, String)>>>>,
}

impl CrateConfig {
	/// Return the file path of this crate.
	pub fn path(&self) -> String {
		crate_name_to_path(&self.name)
	}
}

pub struct Context {
	pub root: tempfile::TempDir,
}

impl Context {
	pub fn new() -> Self {
		Self { root: tempfile::tempdir().expect("Must create a temporary directory") }
	}

	pub fn persist(self) -> PathBuf {
		self.root.into_path()
	}

	pub fn create_crate(&self, module: &CrateConfig) -> Result<(), anyhow::Error> {
		self.cargo(
			&format!("new --vcs=none --offline --lib --name {} {}", module.name, module.path()),
			None,
		)?;
		let toml_path = self.root.path().join(&module.path()).join("Cargo.toml");
		assert!(toml_path.exists(), "Crate must exist");
		// Add the deps
		let mut out_deps = String::from("");
		for dep in module.deps.iter().flatten() {
			out_deps.push_str(&dep.def());
		}

		let mut txt = String::from("[features]\n");
		for (feature, enables) in module.features.iter().flatten() {
			txt.push_str(&format!("{} = [\n", feature));
			for (dep, feat) in enables.iter().flatten() {
				txt.push_str(&format!("\"{}/{}\",\n", dep, feat));
			}
			txt.push_str("]\n");
		}

		let output = format!("{}\n{}", out_deps, txt);
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
/// This is needed for case-insensitive file systems like on MacOS. It prefixes all lower-case
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
	},
}

impl CrateDependency {
	fn def(&self) -> String {
		let option = if self.optional() { ", optional = true".to_string() } else { String::new() };
		let mut ret = match self.rename() {
			Some(rename) => format!("{} = {{ package = \"{}\", ", rename, self.name()),
			None => format!("{} = {{ ", self.name()),
		};
		ret.push_str(&format!("version = \"*\", path = \"../{}\"{}}}\n", self.path(), option));
		ret
	}

	fn path(&self) -> String {
		crate_name_to_path(&self.name())
	}

	fn name(&self) -> String {
		match self {
			Self::Explicit { name, .. } | Self::Implicit(name) => name.clone(),
		}
	}

	fn rename(&self) -> Option<String> {
		match self {
			Self::Explicit { rename, .. } => rename.clone(),
			_ => None,
		}
	}

	fn optional(&self) -> bool {
		match self {
			Self::Explicit { optional, .. } => optional.unwrap_or_default(),
			_ => false,
		}
	}
}


/// Removes leading and trailing empty lines.
pub fn normalize(s: &str) -> String {
	let mut lines = s.lines().collect::<Vec<_>>();
	while lines.first().map(|l| l.is_empty()).is_some() {
		lines.remove(0);
	}
	while lines.last().map(|l| l.is_empty()).is_some() {
		lines.pop();
	}
	format!("{}\n", lines.join("\n"))
}


/// Predicate for serde to skip serialization of default values.
fn is_false(b: &Option<bool>) -> bool {
	!b.unwrap_or_default()
}
