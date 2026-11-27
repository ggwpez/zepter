// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Automatically fix problems by modifying `Cargo.toml` files.

#[cfg(test)]
use crate::cmd::transpose::SourceLocationSelector;
use crate::log;
#[cfg(test)]
use std::collections::BTreeMap as Map;
use std::{
	fmt,
	fmt::Display,
	path::{Path, PathBuf},
};
#[cfg(test)]
use toml_edit::value;
use toml_edit::DocumentMut;

mod dependencies;
mod features;

#[cfg(feature = "logging")]
const LOG_TARGET: &str = module_path!();

#[derive(Debug, clap::Parser)]
#[cfg_attr(feature = "testing", derive(Default))]
pub struct AutoFixerArgs {
	/// Try to automatically fix the problems.
	#[clap(long = "fix")]
	pub enable: bool,
}

pub struct AutoFixer {
	pub manifest: Option<PathBuf>,
	doc: Option<DocumentMut>,
	raw: String,
}

impl AutoFixer {
	pub fn from_manifest<P: AsRef<Path>>(manifest: P) -> Result<Self, String> {
		let raw = std::fs::read_to_string(&manifest)
			.map_err(|e| format!("Failed to read manifest: {e}"))?;
		let doc = raw
			.parse::<DocumentMut>()
			.map_err(|e| format!("Failed to parse manifest: {e}"))?;
		Ok(Self { raw, manifest: Some(manifest.as_ref().to_path_buf()), doc: Some(doc) })
	}

	pub fn from_raw(raw: &str) -> Result<Self, String> {
		let doc = raw
			.parse::<DocumentMut>()
			.map_err(|e| format!("Failed to parse manifest: {e}"))?;
		Ok(Self { raw: raw.into(), manifest: None, doc: Some(doc) })
	}

	pub fn modified(&self) -> bool {
		self.doc.as_ref().unwrap().to_string() != self.raw
	}

	pub fn save(&mut self) -> Result<(), String> {
		let doc = self.doc.as_ref().ok_or("No manifest document to save")?;
		let path = self.manifest.as_ref().ok_or("No manifest path to save")?;
		std::fs::write(path, doc.to_string())
			.map_err(|e| format!("Failed to write manifest: {:?}: {:?}", path.display(), e))?;
		log::debug!("Modified manifest {:?}", path.display());
		Ok(())
	}
}

impl Display for AutoFixer {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.doc.as_ref().unwrap())
	}
}

#[cfg(test)]
mod failure_tests {
	use super::*;

	#[test]
	fn malformed_feature_returns_error() {
		let mut fixer = AutoFixer::from_raw("[features]\ndefault = [1]\n").unwrap();
		assert!(fixer.canonicalize_features("example", &Map::new(), 80).is_err());
		assert!(fixer.remove_feature("dep/").is_err());
	}

	#[test]
	fn empty_feature_value_returns_error() {
		let mut fixer = AutoFixer::from_raw("[features]\ndefault = []\n").unwrap();
		assert!(fixer.add_to_feature("default", "").is_err());
	}

	#[test]
	fn invalid_dependency_version_returns_error() {
		let mut dep = value("not a version");
		assert!(AutoFixer::lift_some_dependency(&mut dep, None, &SourceLocationSelector::Remote)
			.is_err());
	}

	#[test]
	fn saving_without_manifest_returns_error() {
		let mut fixer = AutoFixer::from_raw("").unwrap();
		assert!(fixer.save().is_err());
	}
}
