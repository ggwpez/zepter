// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::AutoFixer;
use crate::{cmd::transpose::SourceLocationSelector, log};
use cargo_metadata::{Dependency, DependencyKind};
use toml_edit::{table, DocumentMut, Formatted, InlineTable, Item, Table, Value};

impl AutoFixer {
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
			log::warn!(target: super::LOG_TARGET, "No '{}' entry found", kind);
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
			cargo_metadata::semver::VersionReq::parse(as_str)
				.map_err(|e| format!("Invalid dependency version '{as_str}': {e}"))?;
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
		let found_orig = deps.get(dep_name);
		let found_rename = maybe_rename.and_then(|r| deps.get(r));

		if found_orig.is_some() && found_rename.is_some() {
			log::warn!(
				target: super::LOG_TARGET,
				"Dependency '{}' exists twice in the workspace: once as '{}' and once as '{}'. Using the alias.",
				dep_name,
				dep_name,
				maybe_rename.unwrap()
			);
		}

		Self::validate_workspace_alias(found_rename, dep_name)?;
		let mut t = Self::existing_workspace_dependency(
			found_rename.or(found_orig),
			dep_name,
			dep_version,
			&version_str,
			default_feats,
			local,
		)?;

		t.insert("version", Value::String(Formatted::new(version_str)));
		if let Some(local) = local {
			t.insert("path", Value::String(Formatted::new(local.into())));
			// Local deps dont need a version.
			t.remove("version");
		}
		if !default_feats {
			t.insert("default-features", Value::Boolean(Formatted::new(default_feats)));
		}

		let name = if let Some(rename) = maybe_rename {
			log::info!(
				target: super::LOG_TARGET,
				"Renaming workspace dependency '{dep_name}' to '{rename}'"
			);
			t.insert("package", Value::String(Formatted::new(dep_name.to_string())));
			rename
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

	fn validate_workspace_alias(found_rename: Option<&Item>, dep_name: &str) -> Result<(), String> {
		if let Some(rename) = found_rename {
			if let Some(rename) = rename.as_table_like() {
				if let Some(pkg) = rename.get("package") {
					let package = pkg.as_str().ok_or_else(|| {
						format!("Dependency '{dep_name}' has a non-string 'package' field")
					})?;
					if package != dep_name {
						return Err(format!(
							"Dependency '{dep_name}' already exists in the workspace with a different alias: '{package}' vs '{dep_name}'"
						))
					}
				} else {
					return Err(format!(
						"Dependency '{dep_name}' already exists in the workspace, but an existing alias in one of the packages has the same name as an un-aliased workspace dependency. This would silently use a different package than expected."
					))
				}
			}
		}
		Ok(())
	}

	fn existing_workspace_dependency(
		found: Option<&Item>,
		dep_name: &str,
		dep_version: &str,
		version_str: &str,
		default_feats: bool,
		local: Option<&str>,
	) -> Result<InlineTable, String> {
		if let Some(found) = found {
			if let Some(found) = found.as_inline_table() {
				if let Some(version) = found.get("version") {
					let version = version.as_str().ok_or_else(|| {
						format!("Dependency '{dep_name}' has a non-string 'version' field")
					})?;
					if remove_carrot(version) != version_str {
						return Err(format!(
							"Dependency '{dep_name}' already exists in the workspace with a different 'version' field: '{version}' vs '{dep_version}'"
						))
					}
				}

				if let Some(local) = local {
					if let Some(path) = found.get("path") {
						let path = path.as_str().ok_or_else(|| {
							format!("Dependency '{dep_name}' has a non-string 'path' field")
						})?;
						let l1 = Self::sanitize_path(local);
						let l2 = Self::sanitize_path(path);

						if l1 != l2 {
							return Err(format!(
								"Dependency '{dep_name}' already exists in the workspace with a different 'path' field: '{local}' vs '{path}'"
							))
						}
					}
				}

				if let Some(default) = found.get("default-features") {
					let default =
						default.as_bool().ok_or_else(|| {
							format!("Dependency '{dep_name}' has a non-boolean 'default-features' field")
						})?;
					if default != default_feats {
						return Err(format!(
							"Dependency '{dep_name}' already exists in the workspace with a different 'default-features' fields: '{default}' vs '{default_feats}'"
						))
					}
				}

				// We checked that:
				// - There is either no version or its compatible
				// - There is either no default-features or its compatible
				return Ok(found.clone())
			} else {
				return Err(format!("Dependency '{dep_name}' already exists in the workspace but could not validate its compatibility"))
			}
		}
		Ok(InlineTable::new())
	}

	fn sanitize_path(p: &str) -> String {
		p.trim_start_matches("./").trim_end_matches("/").to_string()
	}

	pub fn disable_default_features(&mut self, dep: &str) -> Result<(), String> {
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table("dependencies") {
			return Err("No dependencies entry found".into())
		}

		let deps = doc["dependencies"].as_table_mut().unwrap();
		let Some(dep) = deps.get_mut(dep) else {
			return Err(format!("Dependency '{dep}' not found"))
		};

		if let Some(dep) = dep.as_inline_table_mut() {
			dep.insert("default-features", Value::Boolean(Formatted::new(false)));
			Ok(())
		} else {
			Err(format!("Dependency '{dep}' is not an inline table"))
		}
	}
}

fn remove_carrot(version: &str) -> &str {
	version.strip_prefix('^').unwrap_or(version)
}
