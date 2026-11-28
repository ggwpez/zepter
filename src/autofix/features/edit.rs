// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::super::AutoFixer;
use toml_edit::{table, value, Array, DocumentMut, Value};

impl AutoFixer {
	pub fn add_feature(&mut self, feature: &str) -> Result<(), String> {
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table("features") {
			doc.as_table_mut().insert("features", table());
		}
		let features = doc["features"].as_table_mut().unwrap();

		if features.contains_key(feature) {
			return Ok(())
		}

		features.insert(feature, value(Array::new()));
		Ok(())
	}

	/// Add something to a feature. Creates that feature if it does not exist.
	pub fn add_to_feature(&mut self, feature: &str, v: &str) -> Result<(), String> {
		if v.is_empty() {
			return Err("Empty value in feature".into())
		}
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

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

		let feature = features[feature]
			.as_array_mut()
			.ok_or_else(|| format!("Feature '{feature}' is not an array"))?;
		if feature.iter().any(|value| value.as_str().is_none_or(str::is_empty)) {
			return Err("Empty or non-string value in feature".into())
		}
		Self::append_feature_value(feature, v);
		Ok(())
	}

	fn append_feature_value(feature: &mut Array, v: &str) {
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
			// Values were validated before modifying the array.
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
	}

	pub fn remove_feature(&mut self, name: &str) -> Result<(), String> {
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table("features") {
			return Ok(())
		}
		let features = doc["features"].as_table_mut().unwrap();

		for (feature_name, item) in features.iter_mut() {
			let feature = item
				.as_array_mut()
				.ok_or_else(|| format!("Feature '{feature_name}' is not an array"))?;
			if feature.iter().any(|value| value.as_str().is_none()) {
				return Err(format!("Feature '{feature_name}' contains a non-string value"))
			}
			let mut i = 0;
			while i < feature.len() {
				if feature
					.get(i)
					.and_then(Value::as_str)
					.is_some_and(|value| value.starts_with(name))
				{
					feature.remove(i);
				} else {
					i += 1;
				}
			}
		}
		Ok(())
	}
}
