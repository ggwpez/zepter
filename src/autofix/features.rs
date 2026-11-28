// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod edit;
mod format;

use super::AutoFixer;
use crate::{cmd::fmt::Mode, log};
use std::collections::BTreeMap as Map;
use toml_edit::{Array, DocumentMut, Item, Value};

impl AutoFixer {
	// Assumes sorting
	pub fn dedub_feature(cname: &str, fname: &str, feature: &mut Array) -> Result<(), String> {
		let _ = cname;
		let mut values = feature.iter().cloned().collect::<Vec<_>>();

		for i in (0..values.len()).rev() {
			let current = &values[i];
			let cur_str = current
				.as_str()
				.ok_or_else(|| format!("feature '{fname}' contains a non-string value"))?;

			if i == 0 {
				continue
			}
			let last_str = values[i - 1]
				.as_str()
				.ok_or_else(|| format!("feature '{fname}' contains a non-string value"))?;
			if !Self::is_duplicate_feature_value(fname, current, cur_str, last_str)? {
				continue
			}

			values.remove(i);
			log::debug!(
				target: super::LOG_TARGET,
				"Removed duplicate from '{cname}' / '{fname}'"
			);
		}

		feature.clear();
		for value in values {
			feature.push_formatted(value);
		}
		Ok(())
	}

	fn is_duplicate_feature_value(
		fname: &str,
		current: &Value,
		cur_str: &str,
		last_str: &str,
	) -> Result<bool, String> {
		if cur_str < last_str {
			return Err(format!(
				"Cannot de-duplicate: feature is not sorted: {cur_str} < {last_str}"
			))
		}
		if cur_str != last_str {
			if cur_str.chars().filter(|c| *c != '?').eq(last_str.chars().filter(|c| *c != '?')) {
				return Err(format!("feature '{fname}': conflicting ? for '{cur_str}'"))
			}
			return Ok(false)
		}

		// TODO merge the comments
		let prefix = current.decor().prefix().and_then(|p| p.as_str()).unwrap_or("");
		let suffix = current.decor().suffix().and_then(|s| s.as_str()).unwrap_or("");
		if !prefix.trim().is_empty() || !suffix.trim().is_empty() {
			return Err(format!("feature '{fname}': has a comment '{cur_str}'"))
		}
		Ok(true)
	}

	pub fn sort_feature(feature: &mut Array) -> Result<(), String> {
		let mut values = feature.iter().cloned().collect::<Vec<_>>();
		if values.iter().any(|value| value.as_str().is_none()) {
			return Err("feature contains a non-string value".into())
		}
		values.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
		feature.clear();
		for value in values {
			feature.push_formatted(value.clone());
		}
		Ok(())
	}

	pub fn sort_all_features(&mut self) -> Result<(), String> {
		for feature in self.get_all_features() {
			let feature = self
				.get_feature_mut(&feature)
				.map_err(|()| format!("Invalid feature '{feature}'"))?;
			Self::sort_feature(feature)?;
		}

		Ok(())
	}

	pub fn format_all_feature(&mut self, line_width: u32) -> Result<(), String> {
		for fname in self.get_all_features() {
			let feature = self
				.get_feature_mut(&fname)
				.map_err(|()| format!("Invalid feature '{fname}'"))?;
			Self::format_feature(&fname, feature, line_width)?;
		}

		Ok(())
	}

	fn get_all_features(&self) -> Vec<String> {
		let mut found = Vec::new();

		let doc: &DocumentMut = self.doc.as_ref().unwrap();
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
		let doc: &DocumentMut = self.doc.as_ref().unwrap();
		if !doc.contains_table("features") {
			return None
		}
		let features = doc["features"].as_table()?;
		features.get(name)?.as_array()
	}

	pub(crate) fn get_feature_mut(&mut self, name: &str) -> Result<&mut Array, ()> {
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();
		if !doc.contains_table("features") {
			return Err(())
		}
		let features = doc["features"].as_table_mut().ok_or(())?;
		features.get_mut(name).and_then(Item::as_array_mut).ok_or(())
	}

	pub fn canonicalize_feature(
		cname: &str,
		fname: &str,
		modes: &[Mode],
		line_width: u32,
		feature: &mut Array,
	) -> Result<(), String> {
		if modes.contains(&Mode::None) {
			return Ok(())
		}
		if modes.is_empty() || modes.contains(&Mode::Sort) {
			Self::sort_feature(feature)?;
		}
		if modes.is_empty() || modes.contains(&Mode::Dedub) {
			Self::dedub_feature(cname, fname, feature)?;
		}
		if modes.is_empty() || modes.contains(&Mode::Canonicalize) {
			Self::format_feature(fname, feature, line_width)?;
		}
		Ok(())
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
			let Ok(feature) = self.get_feature_mut(fname) else {
				errors.push(format!("Invalid feature '{fname}': expected an array"));
				continue
			};
			let modes = mode_per_feature.get(fname).cloned().unwrap_or_default();

			let _ = Self::canonicalize_feature(cname, fname, &modes, line_width, feature)
				.map_err(|e| errors.push(e));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}

	pub fn is_feature_canonical(
		&self,
		cname: &str,
		fname: &str,
		mode_per_feature: &Map<String, Vec<Mode>>,
		line_width: u32,
	) -> Result<bool, String> {
		let modes = mode_per_feature.get(fname).cloned().unwrap_or_default();

		let orig = self
			.get_feature(fname)
			.ok_or_else(|| format!("Invalid or missing feature '{fname}'"))?;
		let mut modified = orig.clone();

		Self::canonicalize_feature(cname, fname, &modes, line_width, &mut modified)?;
		Ok(orig.to_string() == modified.to_string())
	}
}
