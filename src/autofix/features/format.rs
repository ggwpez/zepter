// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::super::AutoFixer;
use toml_edit::Array;

impl AutoFixer {
	pub fn format_feature(
		fname: &str,
		feature: &mut Array,
		mut line_width: u32,
	) -> Result<(), String> {
		if feature.iter().any(|value| value.as_str().is_none()) {
			return Err(format!("feature '{fname}' contains a non-string value"))
		}
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
			prefix = if prefix.is_empty() { "\n\t".into() } else { format!("\n\t{prefix}\n\t") };
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
			suffix = if suffix.is_empty() { ",\n".into() } else { format!(",\n\t{suffix}\n") };
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
}
