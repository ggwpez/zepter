// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Automatically fix problems by modifying `Cargo.toml` files.

use crate::log;
use std::path::{Path, PathBuf};
use toml_edit::{table, value, Array, Document, Value};

#[derive(Debug, clap::Parser)]
#[cfg_attr(feature = "testing", derive(Default))]
pub struct AutoFixerArgs {
	/// Try to automatically fix the problems.
	#[clap(long = "fix")]
	pub enable: bool,
}

pub struct AutoFixer {
	pub manifest: Option<PathBuf>,
	doc: Option<Document>,
	raw: String,
}

impl AutoFixer {
	pub fn from_manifest(manifest: &Path) -> Result<Self, String> {
		let raw = std::fs::read_to_string(manifest)
			.map_err(|e| format!("Failed to read manifest: {e}"))?;
		let doc = raw.parse::<Document>().map_err(|e| format!("Failed to parse manifest: {e}"))?;
		Ok(Self { raw, manifest: Some(manifest.to_path_buf()), doc: Some(doc) })
	}

	pub fn from_raw(raw: &str) -> Result<Self, String> {
		let doc = raw.parse::<Document>().map_err(|e| format!("Failed to parse manifest: {e}"))?;
		Ok(Self { raw: raw.into(), manifest: None, doc: Some(doc) })
	}

	/// Returns the unsorted features in alphabetical order.
	pub fn check_sorted_all_features(&self) -> Vec<String> {
		let doc: &Document = self.doc.as_ref().unwrap();
		if !doc.contains_table("features") {
			return Vec::new()
		}
		let features = doc["features"].as_table().unwrap();
		let mut unsorted = Vec::new();

		for (feature, _) in features.iter() {
			if !self.check_sorted_feature(feature) {
				unsorted.push(feature.to_string());
			}
		}

		unsorted.sort();
		unsorted
	}

	pub fn check_sorted_feature(&self, feature: &str) -> bool {
		let doc: &Document = self.doc.as_ref().unwrap();
		if !doc.contains_table("features") {
			return true
		}
		let features = doc["features"].as_table().unwrap();
		if !features.contains_key(feature) {
			return true
		}
		let feature = features[feature].as_array().unwrap();

		let mut last = "";
		for value in feature.iter() {
			let value = value.as_str().unwrap();
			if value < last {
				return false
			}
			last = value;
		}
		true
	}

	pub fn sort_all_features(&mut self) -> Result<(), String> {
		let doc: &mut Document = self.doc.as_mut().unwrap();
		if !doc.contains_table("features") {
			return Ok(())
		}
		let features = doc["features"].as_table_mut().unwrap();

		for (_, feature) in features.iter_mut() {
			let feature = feature.as_array_mut().unwrap();
			let mut values = feature.iter().cloned().collect::<Vec<_>>();
			// DOGSHIT CODE
			values.sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
			feature.clear();
			for value in values.into_iter() {
				feature.push_formatted(value.clone());
			}
		}

		Ok(())
	}

	pub fn canonicalize_all_features(&mut self) -> Result<(), String> {
		let doc: &mut Document = self.doc.as_mut().unwrap();
		if !doc.contains_table("features") {
			return Ok(())
		}
		let features = doc["features"].as_table_mut().unwrap();

		for (_, feature) in features.iter_mut() {
			let feature = feature.as_array_mut().unwrap();
			let mut values = feature.iter().cloned().collect::<Vec<_>>();

			for value in values.iter_mut() {
				let mut prefix = value
					.decor()
					.prefix()
					.map_or(String::new(), |p| p.as_str().unwrap().to_string());
				let mut suffix = value
					.decor()
					.suffix()
					.map_or(String::new(), |s| s.as_str().unwrap().to_string());
				suffix = Self::canonicalize_pre_and_suffix(suffix);
				suffix = if suffix.trim().is_empty() {
					"".into()
				} else {
					format!("\n\t{}\n\t", suffix.trim())
				};

				prefix = Self::canonicalize_pre_and_suffix(prefix);
				prefix = prefix.trim().into();
				prefix =
					if prefix.is_empty() { "\n\t".into() } else { format!("\n\t{}\n\t", prefix) };
				value.decor_mut().set_suffix(suffix);
				value.decor_mut().set_prefix(prefix);
			}

			// Last one gets a newline
			if let Some(value) = values.last_mut() {
				let mut suffix = value
					.decor()
					.suffix()
					.map_or(String::new(), |s| s.as_str().unwrap().to_string());

				suffix = Self::canonicalize_pre_and_suffix(suffix);
				suffix = suffix.trim().into();
				suffix =
					if suffix.is_empty() { ",\n".into() } else { format!(",\n\t{}\n", suffix) };
				value.decor_mut().set_suffix(suffix);
			}

			feature.clear();
			for value in values.into_iter() {
				feature.push_formatted(value.clone());
			}
			feature.set_trailing_comma(false);
			feature.set_trailing(feature.trailing().as_str().unwrap().to_string().trim());
			feature.decor_mut().clear();
		}

		Ok(())
	}

	fn canonicalize_pre_and_suffix(fix: String) -> String {
		let lines = fix.lines().collect::<Vec<_>>();
		let mut new_lines = Vec::new();

		for i in 0..lines.len() {
			if i == 0 {
				new_lines.push(lines[i].trim_end().into());
			} else if i == lines.len() - 1 {
				new_lines.push(lines[i].trim_start().into());
			} else {
				new_lines.push(format!("\t{}", lines[i].trim()));
			}
		}

		new_lines.join("\n")
	}

	pub fn format_all_features(&mut self) -> Result<(), String> {
		self.sort_all_features()?;
		self.canonicalize_all_features()?;

		Ok(())
	}

	/// Returns the unsorted features in alphabetical order.
	pub fn check_sorted_all_features(&self) -> Vec<String> {
		let doc: &Document = self.doc.as_ref().unwrap();
		if !doc.contains_table("features") {
			return Vec::new()
		}
		let features = doc["features"].as_table().unwrap();
		let mut unsorted = Vec::new();

		for (feature, _) in features.iter() {
			if !self.check_sorted_feature(feature) {
				unsorted.push(feature.to_string());
			}
		}

		unsorted.sort();
		unsorted
	}

	pub fn check_sorted_feature(&self, feature: &str) -> bool {
		let doc: &Document = self.doc.as_ref().unwrap();
		if !doc.contains_table("features") {
			return true
		}
		let features = doc["features"].as_table().unwrap();
		if !features.contains_key(feature) {
			return true
		}
		let feature = features[feature].as_array().unwrap();

		let mut last = "";
		for value in feature.iter() {
			let value = value.as_str().unwrap();
			if value < last {
				return false
			}
			last = value;
		}
		true
	}

	pub fn sort_all_features(&mut self) -> Result<(), String> {
		let doc: &mut Document = self.doc.as_mut().unwrap();
		if !doc.contains_table("features") {
			return Ok(())
		}
		let features = doc["features"].as_table_mut().unwrap();

		for (_, feature) in features.iter_mut() {
			let feature = feature.as_array_mut().unwrap();
			let mut values = feature.iter().cloned().collect::<Vec<_>>();
			// DOGSHIT CODE
			values.sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
			feature.clear();
			for value in values.into_iter() {
				feature.push_formatted(value.clone());
			}
		}

		Ok(())
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
				prefix = format!("{}\n\t", prefix.trim_end());
			}
			let mut suffix: String = match value.decor().suffix() {
				None => "".into(),
				Some(s) => s.as_str().unwrap().into(),
			};
			suffix = suffix.trim_end_matches('\n').into();
			value.decor_mut().set_suffix(suffix);
			value.decor_mut().set_prefix(prefix.trim_start_matches(' '));
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
			let new_prefix = format!(
				"{}{}",
				new_vals[i - 1].decor().suffix().unwrap().as_str().unwrap(),
				new_vals[i].decor().prefix().unwrap().as_str().unwrap()
			);

			new_vals[i].decor_mut().set_prefix(new_prefix);
			new_vals[i - 1].decor_mut().set_suffix("");
		}

		for new_val in new_vals.into_iter() {
			feature.push_formatted(new_val);
		}

		Ok(())
	}

	pub fn modified(&self) -> bool {
		self.doc.as_ref().unwrap().to_string() != self.raw
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
	use std::vec;

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
	#[case(
		r#"
[features]
runtime-benchmarks = ["sp-runtime/runtime-benchmarks",   "pallet-balances/runtime-benchmarks"]
"#,
		r#"
[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
	"pallet-balances/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
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
	#[case(
		r#"
[features]
runtime-benchmarks = [
"B/F0",
"D/F0",
]
"#,
		r#"
[features]
runtime-benchmarks = [
	"B/F0",
	"D/F0",
	"frame-support/runtime-benchmarks"
]
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

	#[rstest]
	#[case(r#""#, true)]
	#[case(r#"[features]"#, true)]
	#[case(
		r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]"#,
		true
	)]
	#[case(
		r#"
[features]
F0 = [
"B/F0",
"A/F0",
]"#,
		false
	)]
	#[case(
		r#"
[features]
G0 = [
	"B/F0",
	"A/F0",
]"#,
		true
	)]
	#[case(
		r#"
[features]
G0 = [
	"B/F0",
	"A/F0",
]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]"#,
		true
	)]
	#[case(
		r#"
[features]
G0 = [
	"B/F0",
	"A/F0",
]
F0 = [
"B/F0",
"A/F0",
]"#,
		false
	)]
	fn check_sorted_feature_works(#[case] input: &str, #[case] good: bool) {
		let fixer = AutoFixer::from_raw(input).unwrap();
		assert_eq!(fixer.check_sorted_feature("F0"), good);
	}

	#[rstest]
	#[case(r#""#, vec![])]
	#[case(r#"[features]"#, vec![])]
	#[case(r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]"#, vec![])]
	#[case(r#"
[features]
F0 = [
"B/F0",
"A/F0",
]"#, vec!["F0"])]
	#[case(r#"
[features]
G0 = [
	"B/F0",
	"A/F0",
]"#, vec!["G0"])]
	#[case(r#"
[features]
G0 = [
	"B/F0",
	"A/F0",
]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]"#, vec!["G0"])]
	#[case(r#"
[features]
G0 = [
	"B/F0",
	"A/F0",
]
F0 = [
"B/F0",
"A/F0",
]"#, vec!["F0", "G0"])]
	fn check_sorted_all_works(#[case] input: &str, #[case] expect: Vec<&str>) {
		let fixer = AutoFixer::from_raw(input).unwrap();
		assert_eq!(fixer.check_sorted_all_features(), expect);
	}

	#[rstest]
	#[case(r#""#, None)]
	// TODO think about trailing newlines
	#[case(
		r#"[features]"#,
		Some(
			r#"[features]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
	"A/F0",
	"C/F0",
	"B/F0",
]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
	"A/F0",

	"C/F0",
	"B/F0",
]
G0 = [
	"A/G0",
	"C/G0",
	# hi
	"B/G0",
]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
	"B/F0",

	"C/F0",
]
G0 = [
	"A/G0",
	# hi
	"B/G0",
	"C/G0",
]
"#
		)
	)]
	fn sort_all_features_works(#[case] input: &str, #[case] modify: Option<&str>) {
		let mut fixer = AutoFixer::from_raw(input).unwrap();
		fixer.sort_all_features().unwrap();
		assert_eq!(fixer.to_string(), modify.unwrap_or(input));
		assert!(fixer.check_sorted_all_features().is_empty(), "Features should be sorted");
	}

	#[rstest]
	#[case(r#""#, None)]
	#[case(
		r#"[features]"#,
		Some(
			r#"[features]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = ["A/F0"]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
"A/F0"]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
"A/F0"
]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [	"A/F0"	]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [	"A/F0", "B/F0"	]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
		  
  	
	"A/F0",
  
 
	"B/F0"
 	 
]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [	"A/F0",
"B/F0"	]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
    "A/F0",
	"B/F0"	]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = ["A/F0"
	,
	"B/F0"
,
	"C/F0" 
		, ]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
    "A/F0"
  # 1
	,
	"B/F0"
,
	"C/F0" 	,
]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0"
	# 1
	,
	"B/F0",
	"C/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
	
	    # 1    

    "A/F0",
	"B/F0"	]
"#,
		Some(
			r#"
[features]
F0 = [
	# 1
	"A/F0",
	"B/F0",
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
	
	    # 1   

    "A/F0",
	
	 # 2 
	
	"B/F0"

	# 3

		]
"#,
		Some(
			r#"
[features]
F0 = [
	# 1
	"A/F0",
	# 2
	"B/F0",
	# 3
]
"#
		)
	)]
	#[case(
		r#"
[features]
F0 = [
	
	# 1
		# 2  
# 2  

    "A/F0",
	
	 # 3 
	
	"B/F0"

	# 4

		]
"#,
		Some(
			r#"
[features]
F0 = [
	# 1
	# 2
	# 2
	"A/F0",
	# 3
	"B/F0",
	# 4
]
"#
		)
	)]
	#[case(
		r#"
[features]
std = [
        "pallet-election-provider-support-benchmarking?/std",
        "codec/std",
        "scale-info/std",
        "log/std",

        "frame-support/std",
        "frame-system/std",

        "sp-io/std",
        "sp-std/std",
        "sp-core/std",
        "sp-runtime/std",
        "sp-npos-elections/std",
        "sp-arithmetic/std",
        "frame-election-provider-support/std",
        "log/std",

        "frame-benchmarking?/std",
        "rand/std",
        "strum/std",
        "pallet-balances/std",
        "sp-tracing/std"
]"#,
		Some(
			r#"
[features]
std = [
	"pallet-election-provider-support-benchmarking?/std",
	"codec/std",
	"scale-info/std",
	"log/std",
	"frame-support/std",
	"frame-system/std",
	"sp-io/std",
	"sp-std/std",
	"sp-core/std",
	"sp-runtime/std",
	"sp-npos-elections/std",
	"sp-arithmetic/std",
	"frame-election-provider-support/std",
	"log/std",
	"frame-benchmarking?/std",
	"rand/std",
	"strum/std",
	"pallet-balances/std",
	"sp-tracing/std",
]
"#
		)
	)]
	// FIXME: Spaces before the = are not removed.
	#[case(
		r#"
[features]
F0 =  	[  "A/F0"	, 	"B/F0"	]
"#,
		Some(
			r#"
[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
		)
	)]
	fn canonicalize_all_features_works(#[case] input: &str, #[case] modify: Option<&str>) {
		let mut fixer = AutoFixer::from_raw(input).unwrap();
		fixer.canonicalize_all_features().unwrap();
		pretty_assertions::assert_str_eq!(fixer.to_string(), modify.unwrap_or(input));
	}
}
