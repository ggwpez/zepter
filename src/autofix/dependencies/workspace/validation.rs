// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::super::super::AutoFixer;
use toml_edit::{InlineTable, Item};

impl AutoFixer {
	pub(super) fn validate_workspace_alias(
		found_rename: Option<&Item>,
		dep_name: &str,
	) -> Result<(), String> {
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

	pub(super) fn existing_workspace_dependency(
		found: Option<&Item>,
		dep_name: &str,
		dep_version: &str,
		default_feats: bool,
		local: Option<&str>,
	) -> Result<InlineTable, String> {
		let Some(found) = found else { return Ok(InlineTable::new()) };
		let found = found.as_inline_table().ok_or_else(|| {
			format!("Dependency '{dep_name}' already exists in the workspace but could not validate its compatibility")
		})?;
		Self::validate_workspace_version(found, dep_name, dep_version)?;
		Self::validate_workspace_path(found, dep_name, local)?;
		if let Some(default) = found.get("default-features") {
			let default = default.as_bool().ok_or_else(|| {
				format!("Dependency '{dep_name}' has a non-boolean 'default-features' field")
			})?;
			if default != default_feats {
				return Err(format!(
					"Dependency '{dep_name}' already exists in the workspace with a different 'default-features' fields: '{default}' vs '{default_feats}'"
				))
			}
		}
		Ok(found.clone())
	}

	fn validate_workspace_version(
		found: &InlineTable,
		dep_name: &str,
		dep_version: &str,
	) -> Result<(), String> {
		if let Some(version) = found.get("version") {
			let version = version.as_str().ok_or_else(|| {
				format!("Dependency '{dep_name}' has a non-string 'version' field")
			})?;
			if remove_carrot(version) != dep_version.trim_start_matches('^') {
				return Err(format!(
					"Dependency '{dep_name}' already exists in the workspace with a different 'version' field: '{version}' vs '{dep_version}'"
				))
			}
		}
		Ok(())
	}

	fn validate_workspace_path(
		found: &InlineTable,
		dep_name: &str,
		local: Option<&str>,
	) -> Result<(), String> {
		if let Some(local) = local {
			if let Some(path) = found.get("path") {
				let path = path.as_str().ok_or_else(|| {
					format!("Dependency '{dep_name}' has a non-string 'path' field")
				})?;
				if Self::sanitize_path(local) != Self::sanitize_path(path) {
					return Err(format!(
						"Dependency '{dep_name}' already exists in the workspace with a different 'path' field: '{local}' vs '{path}'"
					))
				}
			}
		}
		Ok(())
	}

	fn sanitize_path(p: &str) -> &str {
		p.trim_start_matches("./").trim_end_matches("/")
	}
}

fn remove_carrot(version: &str) -> &str {
	version.strip_prefix('^').unwrap_or(version)
}
