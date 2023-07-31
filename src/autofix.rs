// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Automatically fix problems by modifying `Cargo.toml` files.

use std::path::{Path, PathBuf};
use toml_edit::{table, value, Array, Document, Value};

pub struct AutoFixer {
	pub manifest: Option<PathBuf>,
	doc: Option<Document>,
}

impl AutoFixer {
	pub fn from_manifest(manifest: &Path) -> Result<Self, String> {
		let raw = std::fs::read_to_string(manifest)
			.map_err(|e| format!("Failed to read manifest: {e}"))?;
		let doc = raw.parse::<Document>().map_err(|e| format!("Failed to parse manifest: {e}"))?;
		Ok(Self { manifest: Some(manifest.to_path_buf()), doc: Some(doc) })
	}

	pub fn from_raw(raw: &str) -> Result<Self, String> {
		let doc = raw.parse::<Document>().map_err(|e| format!("Failed to parse manifest: {e}"))?;
		Ok(Self { manifest: None, doc: Some(doc) })
	}

	/// Add something to a feature. Creates that feature if it does not exist.
	pub fn add_to_feature(&mut self, feature: &str, v: &str) -> Result<(), String> {
		let doc: &mut Document = self.doc.as_mut().unwrap();

		if !doc.contains_table("features") {
			doc.as_table_mut().insert("features", table());
		}
		let features = doc["features"].as_table_mut().unwrap();

		if !features.contains_key(feature) {
			features.insert(feature, table());
		}
		if !features.contains_value(feature) {
			features.insert(feature, value(Array::new()));
		}

		let feature = features[feature].as_array_mut().unwrap();
		// Lets format them while were at it, otherwise you will end up with `feature = [… very long
		// line …]`.
		let values = feature.iter().cloned().collect::<Vec<_>>();
		feature.clear();
		feature.set_trailing(
			feature.trailing().as_str().unwrap().to_string().trim_start_matches('\n'),
		);
		feature.set_trailing_comma(false); // We need to add this manually later on.
		let mut new_vals = Vec::new();

		for mut value in values.into_iter() {
			if value.as_str().map_or(false, |s| s.is_empty()) {
				panic!("Empty value in feature");
			}
			let mut prefix: String = match value.decor().prefix() {
				None => "".into(),
				Some(p) => p.as_str().unwrap().into(),
			};
			if !prefix.ends_with("\n\t") {
				prefix = format!("{prefix}\n\t");
			}
			let mut suffix: String = match value.decor().suffix() {
				None => "".into(),
				Some(s) => s.as_str().unwrap().into(),
			};
			suffix = suffix.trim_end_matches('\n').into();
			value.decor_mut().set_suffix(suffix);
			value.decor_mut().set_prefix(prefix);
			new_vals.push(value);
		}

		if v.is_empty() {
			unreachable!("Empty value in feature");
		}
		let mut value: Value = v.into();
		let suffix = "\n";
		value = value.decorated("\n\t", suffix);
		new_vals.push(value);

		for i in 1..new_vals.len() {
			let new_prefix = format!("{}{}", new_vals[i - 1].decor().suffix().unwrap().as_str().unwrap(), new_vals[i].decor().prefix().unwrap().as_str().unwrap());

			new_vals[i].decor_mut().set_prefix(new_prefix);
			new_vals[i - 1].decor_mut().set_suffix("");
		}

		for new_val in new_vals.into_iter() {
			feature.push_formatted(new_val);
		}

		Ok(())
	}

	pub fn save(&mut self) -> Result<(), String> {
		if let (Some(doc), Some(path)) = (self.doc.take(), &self.manifest) {
			std::fs::write(path, doc.to_string())
				.map_err(|e| format!("Failed to write manifest: {:?}: {:?}", path.display(), e))?;
			log::info!("Modified manifest {:?}", path.display());
		}
		Ok(())
	}
}

impl ToString for AutoFixer {
	fn to_string(&self) -> String {
		self.doc.as_ref().unwrap().to_string()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::*;

	#[rstest]
	// Keeps comments
	#[case(
		r#"
[features]
runtime-benchmarks = [
	# TOML comments are preserved
	"sp-runtime/runtime-benchmarks"
]
"#,
		r#"
[features]
runtime-benchmarks = [
	# TOML comments are preserved
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
	)]
	// Keeps newlines
	#[case(
		r#"
[features]
runtime-benchmarks = [
	
	"sp-runtime/runtime-benchmarks"
]
"#,
		r#"
[features]
runtime-benchmarks = [
	
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
	)]
	// Keeps newlines 2
	#[case(
		r#"
[features]
runtime-benchmarks = [
	"pallet-balances/runtime-benchmarks",
	
	
	"sp-runtime/runtime-benchmarks"
]
"#,
		r#"
[features]
runtime-benchmarks = [
	"pallet-balances/runtime-benchmarks",
	
	
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
	)]
	// Keeps newlines and comments
	#[case(
		r#"
# 1
[features]
# 2
runtime-benchmarks = [
	# 3
	"pallet-balances/runtime-benchmarks",
	# 4
	
	# 5
	"sp-runtime/runtime-benchmarks"
	# 6
]
# 7
"#,
		r#"
# 1
[features]
# 2
runtime-benchmarks = [
	# 3
	"pallet-balances/runtime-benchmarks",
	# 4
	
	# 5
	"sp-runtime/runtime-benchmarks",
	# 6
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
# 7
"#
	)]
	#[case(
		r#"
[features]
runtime-benchmarks = ["sp-runtime/runtime-benchmarks"]
"#,
		r#"
[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
	)]
	#[case(
		r#"
[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks"
]
"#,
		r#"
[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
	)]
	#[case(
		r#"
[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
]
"#,
		r#"
[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
	)]
	#[case(
		r#"
[features]
runtime-benchmarks = []
"#,
		r#"
[features]
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
	)]
	#[case(
		r#"
[package]
name = "something"

[features]
runtime-benchmarks = []
std = ["frame-support/std"]
"#,
		r#"
[package]
name = "something"

[features]
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-support/std",
	"frame-system/std"
]
"#
	)]
	fn add_to_features_works(#[case] before: &str, #[case] after: &str) {
		let mut fixer = AutoFixer::from_raw(before).unwrap();
		fixer
			.add_to_feature("runtime-benchmarks", "frame-support/runtime-benchmarks")
			.unwrap();
		fixer.add_to_feature("std", "frame-system/std").unwrap();
		assert_eq!(fixer.to_string(), after);
	}

	#[rstest]
	#[case(
		r#"
[features]
runtime-benchmarks = [
	# Inside empty works
]
"#,
		r#"
[features]
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
	# Inside empty works
]
"#
	)]
	#[case(
		r#"
[features]
runtime-benchmarks = [
	# TOML comments are preserved
	"sp-runtime/runtime-benchmarks"
]
"#,
		r#"
[features]
runtime-benchmarks = [
	# TOML comments are preserved
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
"#
	)]
	#[case(
		r#"
[features]
# TOML comments are preserved
runtime-benchmarks = []
"#,
		r#"
[features]
# TOML comments are preserved
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
]
"#
	)]
	#[case(
		r#"
# First comment
[features]
# Second comment
runtime-benchmarks = []
"#,
		r#"
# First comment
[features]
# Second comment
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
]
"#
	)]
	#[case(
		r#"
# First comment
[features]
# Second comment
runtime-benchmarks = [
	# Third comment
	"sp-runtime/runtime-benchmarks",
	# Fourth comment
]
# Fifth comment
"#,
		r#"
# First comment
[features]
# Second comment
runtime-benchmarks = [
	# Third comment
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
	# Fourth comment
]
# Fifth comment
"#
	)]
	fn add_feature_keeps_comments(#[case] before: &str, #[case] after: &str) {
		let mut fixer = AutoFixer::from_raw(before).unwrap();
		fixer
			.add_to_feature("runtime-benchmarks", "frame-support/runtime-benchmarks")
			.unwrap();
		assert_eq!(fixer.to_string(), after);
	}

	#[test]
	fn crate_feature_works_without_section_exists() {
		let before = r#""#;
		let after = r#"[features]
std = [
	"AAA",
	"BBB",
	"CCC"
]
"#;
		let mut fixer = AutoFixer::from_raw(before).unwrap();
		fixer.add_to_feature("std", "AAA").unwrap();
		fixer.add_to_feature("std", "BBB").unwrap();
		fixer.add_to_feature("std", "CCC").unwrap();
		assert_eq!(fixer.to_string(), after);
	}

	#[test]
	fn add_to_feature_keeps_format() {
		let raw = std::fs::read_to_string("Cargo.toml").unwrap();
		let fixer = AutoFixer::from_raw(&raw).unwrap();
		assert_eq!(fixer.to_string(), raw, "Formatting stays");
	}
}
