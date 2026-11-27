// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{crate_name_to_path, ModuleName};
use std::{
	collections::{BTreeMap, HashMap},
	fs,
	io::Write,
	path::PathBuf,
	process::Command,
};

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

	fn manifest_sections(&self) -> String {
		// Add the deps
		let mut out_deps = HashMap::<cargo_metadata::DependencyKind, String>::new();
		for dep in self.deps.iter().flatten() {
			out_deps.entry(dep.kind()).or_default().push_str(&dep.def());
		}

		let mut txt = String::from("[features]\n");
		for (feature, enables) in self.features.iter().flatten() {
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

		format!("{deps}\n{txt}")
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
		let output = module.manifest_sections();
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

/// Predicate for serde to skip serialization of default values.
fn is_false(b: &Option<bool>) -> bool {
	!b.unwrap_or_default()
}
