use std::path::{Path, PathBuf};
use toml_edit::{table, value, Array, Document, Value};

pub struct AutoFixer {
	pub manifest: Option<PathBuf>,
	doc: Option<Document>,
}

impl AutoFixer {
	pub fn from_manifest(manifest: &Path) -> Result<Self, String> {
		let raw = std::fs::read_to_string(manifest)
			.map_err(|e| format!("Failed to read manifest: {}", e))?;
		let doc = raw
			.parse::<Document>()
			.map_err(|e| format!("Failed to parse manifest: {}", e))?;
		Ok(Self { manifest: Some(manifest.to_path_buf()), doc: Some(doc) })
	}

	pub fn from_raw(raw: &str) -> Result<Self, String> {
		let doc = raw
			.parse::<Document>()
			.map_err(|e| format!("Failed to parse manifest: {}", e))?;
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

		for value in values.into_iter() {
			if value.as_str().map_or(false, |s| s.is_empty()) {
				panic!("Empty value in feature");
			}
			let value = value.decorated("\n\t", "");
			feature.push_formatted(value);
		}
		if v.is_empty() {
			panic!("Empty value in feature");
		}
		let mut value: Value = v.into();
		// Working around `feature = []`.
		value = value.decorated("\n\t", "\n");
		feature.push_formatted(value);

		Ok(())
	}

	pub fn save(&mut self) -> Result<(), String> {
		if let (Some(doc), Some(path)) = (self.doc.take(), &self.manifest) {
			std::fs::write(&path, doc.to_string())
				.map_err(|e| format!("Failed to write manifest: {}", e))?;
			log::warn!("Wrote manifest to {}", path.display());
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

	#[test]
	fn add_to_feature_works() {
		let before = r#"
[package]
name = "something"

[features]
runtime-benchmarks = []
std = ["frame-support/std"]
    "#;

		let after = r#"
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
    "#;

		let mut fixer = AutoFixer::from_raw(before).unwrap();
		fixer
			.add_to_feature("runtime-benchmarks", "frame-support/runtime-benchmarks")
			.unwrap();
		fixer.add_to_feature("std", "frame-system/std").unwrap();
		assert_eq!(fixer.to_string(), after);
	}

	#[test]
	fn crate_feature_works_without_section_exists() {
		let before = r#""#;
		let after = r#"[features]
std = [
	"AAA",
	"BBB"
]
"#;
		let mut fixer = AutoFixer::from_raw(before).unwrap();
		fixer.add_to_feature("std", "AAA").unwrap();
		fixer.add_to_feature("std", "BBB").unwrap();
		assert_eq!(fixer.to_string(), after);
	}

	#[test]
	fn add_to_feature_keeps_format() {
		let raw = std::fs::read_to_string("Cargo.toml").unwrap();
		let fixer = AutoFixer::from_raw(&raw).unwrap();
		assert_eq!(fixer.to_string(), raw, "Formatting stays");
	}
}
