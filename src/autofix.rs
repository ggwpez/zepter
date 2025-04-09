// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Automatically fix problems by modifying `Cargo.toml` files.

use crate::{
	cmd::{fmt::Mode, transpose::SourceLocationSelector},
	log,
};
use cargo_metadata::{Dependency, DependencyKind};
use std::{
	collections::BTreeMap as Map,
	fmt,
	fmt::Display,
	path::{Path, PathBuf},
};
use toml_edit::{table, value, Array, DocumentMut, Formatted, InlineTable, Item, Table, Value};

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

	// Assumes sorting
	pub fn dedub_feature(cname: &str, fname: &str, feature: &mut Array) -> Result<(), String> {
		let _ = cname;
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
				log::debug!("Removed duplicate from '{cname}' / '{fname}'");
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
		let features = doc["features"].as_table().unwrap();

		if !features.contains_key(name) {
			return None
		}

		Some(features[name].as_array().unwrap())
	}

	pub(crate) fn get_feature_mut(&mut self, name: &str) -> Result<&mut Array, ()> {
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();
		if !doc.contains_table("features") {
			return Err(())
		}
		let features = doc["features"].as_table_mut().unwrap();

		if !features.contains_key(name) {
			return Err(())
		}

		Ok(features[name].as_array_mut().unwrap())
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
			Self::sort_feature(feature);
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
			let feature = self.get_feature_mut(fname).unwrap();
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

		let orig = self.get_feature(fname).unwrap();
		let mut modified = orig.clone();

		Self::canonicalize_feature(cname, fname, &modes, line_width, &mut modified)?;
		Ok(orig.to_string() == modified.to_string())
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
			if value.as_str().is_some_and(|s| s.is_empty()) {
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

	pub fn lift_dependency(
		&mut self,
		dname: &str,
		kind: &DependencyKind,
		default_feats: Option<bool>,
		location: &SourceLocationSelector,
	) -> Result<(), String> {
		let kind = crate::kind_to_str(kind);
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table(kind) {
			log::warn!("No '{}' entry found", kind);
			return Ok(())
		}
		let deps: &mut Table = doc[kind].as_table_mut().unwrap();

		if !deps.contains_key(dname) {
			return Ok(())
		}

		let dep = deps.get_mut(dname).unwrap();
		Self::lift_some_dependency(dep, default_feats, location)?;

		Ok(())
	}

	pub fn lift_some_dependency(
		dep: &mut Item,
		default_feats: Option<bool>,
		location: &SourceLocationSelector,
	) -> Result<(), String> {
		if let Some(as_str) = dep.as_str() {
			cargo_metadata::semver::VersionReq::parse(as_str).expect("Is semver");
			let mut table = InlineTable::new();

			table.insert("workspace", Value::Boolean(Formatted::new(true)));
			if let Some(default_feats) = default_feats {
				table.insert("default-features", Value::Boolean(Formatted::new(default_feats)));
			} else {
				table.remove("default-features");
			}

			// Workspace dependencies ignore aliases as they need to be set in the workspace.
			table.remove("package");
			table.set_dotted(false);

			*dep = Item::Value(Value::InlineTable(table));
		} else if let Some(as_table) = dep.as_inline_table_mut() {
			if as_table.contains_key("git") {
				return Err("Cannot lift git dependencies".into())
			}

			match location {
				SourceLocationSelector::Remote =>
					if as_table.contains_key("path") {
						return Err("Lifting dependency would change it from a path dependency to a crates-io dependency".into())
					},
				SourceLocationSelector::Local =>
					if as_table.contains_key("version") {
						return Err("Lifting dependency would change it from a crates-io dependency to a local dependency".into())
					},
			}

			as_table.remove("path");
			as_table.remove("version");
			as_table.remove("package");

			as_table.insert("workspace", Value::Boolean(Formatted::new(true)));
			if let Some(default_feats) = default_feats {
				as_table.insert("default-features", Value::Boolean(Formatted::new(default_feats)));
			} else {
				as_table.remove("default-features");
			}
		} else {
			return Err("Dependency is not a string or an inline table".into())
		}
		Ok(())
	}

	pub fn add_workspace_dep(
		&mut self,
		dep: &Dependency,
		maybe_rename: Option<&str>,
		default_feats: bool,
		local: Option<&str>,
	) -> Result<(), String> {
		self.add_workspace_dep_inner(
			&dep.name,
			maybe_rename,
			&dep.req.to_string(),
			default_feats,
			local,
		)
	}

	pub(crate) fn add_workspace_dep_inner(
		&mut self,
		dep_name: &str,
		maybe_rename: Option<&str>,
		dep_version: &str,
		default_feats: bool,
		local: Option<&str>,
	) -> Result<(), String> {
		// The carrot is implicit in cargo.
		let version_str = dep_version.to_string().trim_start_matches('^').to_string();
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table("workspace") {
			return Err("No workspace entry found".into())
		}
		let workspace = doc["workspace"].as_table_mut().unwrap();

		if !workspace.contains_table("dependencies") {
			workspace.insert("dependencies", table());
		}

		let deps = workspace["dependencies"].as_table_mut().unwrap();
		let mut t = InlineTable::new();

		let found_orig = deps.get(dep_name);
		let found_rename = maybe_rename.and_then(|r| deps.get(r));

		if found_orig.is_some() && found_rename.is_some() {
			log::warn!(
				"Dependency '{}' exists twice in the workspace: once as '{}' and once as '{}'. Using the alias.",
				dep_name,
				dep_name,
				maybe_rename.unwrap()
			);
		}

		if let Some(rename) = found_rename {
			if let Some(rename) = rename.as_table_like() {
				if let Some(pkg) = rename.get("package") {
					if pkg.as_str().unwrap() != dep_name {
						return Err(format!(
							"Dependency '{}' already exists in the workspace with a different alias: '{}' vs '{}'",
							dep_name,
							pkg.as_str().unwrap(),
							dep_name
						))
					}
				} else {
					return Err(format!(
						"Dependency '{}' already exists in the workspace, but an existing alias in one of the packages has the same name as an un-aliased workspace dependency. This would silently use a different package than expected.",
						dep_name
					))
				}
			}
		}

		if let Some(found) = found_rename.or(found_orig) {
			if let Some(found) = found.as_inline_table() {
				if let Some(version) = found.get("version") {
					if remove_carrot(version.as_str().unwrap()) != version_str {
						return Err(format!(
							"Dependency '{}' already exists in the workspace with a different 'version' field: '{}' vs '{}'",
							dep_name,
							version.as_str().unwrap(),
							dep_version
						))
					}
				}

				if let Some(local) = local {
					if let Some(path) = found.get("path") {
						let l1 = Self::sanitize_path(local);
						let l2 = Self::sanitize_path(path.as_str().unwrap());

						if l1 != l2 {
							return Err(format!(
								"Dependency '{}' already exists in the workspace with a different 'path' field: '{}' vs '{}'",
								dep_name,
								local,
								path.as_str().unwrap()
							))
						}
					}
				}

				if let Some(default) = found.get("default-features") {
					if default.as_bool().unwrap() != default_feats {
						return Err(format!(
							"Dependency '{}' already exists in the workspace with a different 'default-features' fields: '{}' vs '{}'",
							dep_name,
							default.as_bool().unwrap(),
							default_feats
						))
					}
				}

				// We checked that:
				// - There is either no version or its compatible
				// - There is either no default-features or its compatible
				t = found.clone();
			} else {
				return Err(format!("Dependency '{}' already exists in the workspace but could not validate its compatibility", dep_name))
			}
		}

		t.insert("version", Value::String(Formatted::new(version_str)));
		if let Some(local) = local {
			t.insert("path", Value::String(Formatted::new(local.into())));
			// Local deps dont need a version.
			t.remove("version");
		}
		if !default_feats {
			t.insert("default-features", Value::Boolean(Formatted::new(default_feats)));
		}

		let name = if maybe_rename.is_some() {
			log::info!(
				"Renaming workspace dependency '{}' to '{}'",
				dep_name,
				maybe_rename.unwrap()
			);
			t.insert("package", Value::String(Formatted::new(dep_name.to_string())));
			maybe_rename.unwrap()
		} else {
			dep_name
		};

		let new_name = maybe_rename.unwrap_or(name);
		if dep_name != new_name {
			deps.remove(dep_name);
		}
		deps.insert(new_name, Item::Value(Value::InlineTable(t)));

		Ok(())
	}

	fn sanitize_path(p: &str) -> String {
		p.trim_start_matches("./").trim_end_matches("/").to_string()
	}

	pub fn remove_feature(&mut self, name: &str) {
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table("features") {
			return
		}
		let features = doc["features"].as_table_mut().unwrap();

		for feature in features.iter_mut() {
			let feature = feature.1.as_array_mut().unwrap();

			// remove all values that start with `name`
			let mut i = 0;
			while i < feature.len() {
				let value = feature.get(i).unwrap().as_str().unwrap();
				if value.starts_with(name) {
					feature.remove(i);
				} else {
					i += 1;
				}
			}
		}
	}

	pub fn disable_default_features(&mut self, dep: &str) -> Result<(), String> {
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table("dependencies") {
			return Err("No dependencies entry found".into())
		}

		let deps = doc["dependencies"].as_table_mut().unwrap();
		let Some(dep) = deps.get_mut(dep) else {
			return Err(format!("Dependency '{}' not found", dep))
		};

		if let Some(dep) = dep.as_inline_table_mut() {
			dep.insert("default-features", Value::Boolean(Formatted::new(false)));
			Ok(())
		} else {
			Err(format!("Dependency '{}' is not an inline table", dep))
		}
	}

	pub fn modified(&self) -> bool {
		self.doc.as_ref().unwrap().to_string() != self.raw
	}

	pub fn save(&mut self) -> Result<(), String> {
		if let (Some(doc), Some(path)) = (self.doc.take(), &self.manifest) {
			std::fs::write(path, doc.to_string())
				.map_err(|e| format!("Failed to write manifest: {:?}: {:?}", path.display(), e))?;
			log::debug!("Modified manifest {:?}", path.display());
		}
		Ok(())
	}
}

impl Display for AutoFixer {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.doc.as_ref().unwrap())
	}
}

fn remove_carrot(version: &str) -> &str {
	version.strip_prefix('^').unwrap_or(version)
}
