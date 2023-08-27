// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Automatically fix problems by modifying `Cargo.toml` files.

use crate::{cmd::fmt::Mode, log};
use std::{
	collections::BTreeMap as Map,
	path::{Path, PathBuf},
};
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
		let Some(feature) = self.get_feature(feature) else { return true };

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

	// Assumes sorting
	pub fn dedub_feature(cname: &str, fname: &str, feature: &mut Array) -> Result<(), String> {
		let mut values = feature.iter().cloned().collect::<Vec<_>>();

		for i in (0..values.len()).rev() {
			let last: Option<Value> = if i == 0 { None } else { Some(values[i - 1].clone()) };
			let current = &values[i];
			let cur_str = current.as_str().unwrap();

			if let Some(ref last) = last {
				let last_str = last.as_str().unwrap();
				if cur_str < last_str {
					return Err(format!(
						"Cannot de-duplicate: feature is not sorted: {} < {}",
						cur_str, last_str
					))
				}

				if cur_str != last_str {
					if cur_str.replace('?', "") == last_str.replace('?', "") {
						return Err(format!("feature '{fname}': conflicting ? for '{cur_str}'"))
					}
					continue
				}

				// TODO merge the comments
				let prefix = current.decor().prefix().unwrap().as_str().unwrap();
				let suffix = current.decor().suffix().unwrap().as_str().unwrap();
				if !prefix.trim().is_empty() || !suffix.trim().is_empty() {
					return Err(format!("feature '{fname}': has a comment '{cur_str}'"))
				}

				values.remove(i);
				log::info!("Removed duplicate from '{cname}' / '{fname}'");
			}
		}

		feature.clear();
		for value in values {
			feature.push_formatted(value.clone());
		}
		Ok(())
	}

	pub fn sort_feature(feature: &mut Array) {
		let mut values = feature.iter().cloned().collect::<Vec<_>>();
		// DOGSHIT CODE
		values.sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
		feature.clear();
		for value in values {
			feature.push_formatted(value.clone());
		}
	}

	pub fn sort_all_features(&mut self) -> Result<(), String> {
		for feature in self.get_all_features() {
			let feature = self.get_feature_mut(&feature).unwrap();
			Self::sort_feature(feature);
		}

		Ok(())
	}

	pub fn format_all_feature(&mut self, line_width: u32) -> Result<(), String> {
		for fname in self.get_all_features() {
			let feature = self.get_feature_mut(&fname).unwrap();
			Self::format_feature(&fname, feature, line_width)?;
		}

		Ok(())
	}

	pub fn format_feature(
		fname: &str,
		feature: &mut Array,
		mut line_width: u32,
	) -> Result<(), String> {
		// First we try to format it into one line.
		let mut oneliner = feature.clone();
		if Self::format_feature_oneline(&mut oneliner).is_ok() {
			// Best effort: +1 for the space
			line_width = line_width.saturating_sub(fname.len() as u32 + 1);

			if oneliner.to_string().len() < line_width as usize {
				*feature = oneliner;
				return Ok(())
			}
		}

		// Then we try to format it into multiple lines.
		Self::format_feature_multiline(feature)
	}

	/// Try to canonicalize into one line.
	///
	/// Can fail when there are comments in the way.
	pub fn format_feature_oneline(feature: &mut Array) -> Result<(), String> {
		let mut values = feature.iter().cloned().collect::<Vec<_>>();

		// First we do the checking, then the modify.
		if !feature.trailing().as_str().unwrap().trim().is_empty() {
			return Err("has trailing".into())
		}

		for value in values.iter() {
			let decor = value.decor();
			// Spaghetti code
			let prefix = decor
				.prefix()
				.map(|p| {
					p.as_str().unwrap().chars().filter(|c| !c.is_whitespace()).collect::<String>()
				})
				.unwrap_or_default();
			let suffix = decor
				.suffix()
				.map(|s| {
					s.as_str().unwrap().chars().filter(|c| !c.is_whitespace()).collect::<String>()
				})
				.unwrap_or_default();

			if !prefix.is_empty() || !suffix.is_empty() {
				return Err("has comments".into())
			}
		}

		// Now we modify
		for value in values.iter_mut() {
			value.decor_mut().clear();
			value.decor_mut().set_prefix(" ");
		}

		if let Some(last) = values.last_mut() {
			last.decor_mut().set_suffix(" ");
		}

		feature.set_trailing_comma(false);
		feature.set_trailing("");
		feature.clear();
		for value in values {
			feature.push_formatted(value.clone());
		}

		Ok(())
	}

	pub fn format_feature_multiline(feature: &mut Array) -> Result<(), String> {
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
			suffix = Self::format_pre_and_suffix(suffix);
			suffix = if suffix.trim().is_empty() {
				"".into()
			} else {
				format!("\n\t{}\n\t", suffix.trim())
			};

			prefix = Self::format_pre_and_suffix(prefix);
			prefix = prefix.trim().into();
			prefix = if prefix.is_empty() { "\n\t".into() } else { format!("\n\t{}\n\t", prefix) };
			value.decor_mut().set_suffix(suffix);
			value.decor_mut().set_prefix(prefix);
		}

		// Last one gets a newline
		if let Some(value) = values.last_mut() {
			let mut suffix = value
				.decor()
				.suffix()
				.map_or(String::new(), |s| s.as_str().unwrap().to_string());

			suffix = Self::format_pre_and_suffix(suffix);
			suffix = suffix.trim().into();
			suffix = if suffix.is_empty() { ",\n".into() } else { format!(",\n\t{}\n", suffix) };
			value.decor_mut().set_suffix(suffix);
		}

		feature.clear();
		for value in values {
			feature.push_formatted(value.clone());
		}
		feature.set_trailing_comma(false);
		feature.set_trailing(feature.trailing().as_str().unwrap().to_string().trim());
		feature.decor_mut().clear();

		Ok(())
	}

	fn get_all_features(&self) -> Vec<String> {
		let mut found = Vec::new();

		let doc: &Document = self.doc.as_ref().unwrap();
		if !doc.contains_table("features") {
			return found
		}
		let features = doc["features"].as_table().unwrap();

		for (feature, _) in features.iter() {
			found.push(feature.into());
		}

		found
	}

	fn get_feature(&self, name: &str) -> Option<&Array> {
		let doc: &Document = self.doc.as_ref().unwrap();
		if !doc.contains_table("features") {
			return None
		}
		let features = doc["features"].as_table().unwrap();

		if !features.contains_key(name) {
			return None
		}

		Some(features[name].as_array().unwrap())
	}

	pub(crate) fn get_feature_mut(&mut self, name: &str) -> Result<&mut Array, ()> {
		let doc: &mut Document = self.doc.as_mut().unwrap();
		if !doc.contains_table("features") {
			return Err(())
		}
		let features = doc["features"].as_table_mut().unwrap();

		if !features.contains_key(name) {
			return Err(())
		}

		Ok(features[name].as_array_mut().unwrap())
	}

	pub fn canonicalize_features(
		&mut self,
		cname: &str,
		mode_per_feature: &Map<String, Vec<Mode>>,
		line_width: u32,
	) -> Result<(), Vec<String>> {
		let features = self.get_all_features();
		let mut errors = Vec::new();

		for fname in features.iter() {
			let feature = self.get_feature_mut(fname).unwrap();
			let modes = mode_per_feature.get(fname).cloned().unwrap_or_default();

			if modes.contains(&Mode::None) {
				continue
			}
			if modes.is_empty() || modes.contains(&Mode::Sort) {
				Self::sort_feature(feature);
			}
			if modes.is_empty() || modes.contains(&Mode::Dedub) {
				let _ = Self::dedub_feature(cname, fname, feature).map_err(|e| {
					errors.push(e);
				});
			}
			if modes.is_empty() || modes.contains(&Mode::Canonicalize) {
				let _ = Self::format_feature(fname, feature, line_width).map_err(|e| {
					errors.push(e);
				});
			}
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}

	fn format_pre_and_suffix(fix: String) -> String {
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

	pub fn canonicalize_all_features(&mut self, line_width: u32) -> Result<(), String> {
		self.sort_all_features()?;
		self.format_all_feature(line_width)?;

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

		for mut value in values {
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

		for new_val in new_vals {
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
