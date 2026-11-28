// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod validation;

use super::super::AutoFixer;
use crate::log;
use cargo_metadata::Dependency;
use toml_edit::{table, DocumentMut, Formatted, Item, Value};

impl AutoFixer {
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
		let version_str = dep_version.trim_start_matches('^').to_string();
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table("workspace") {
			return Err("No workspace entry found".into())
		}
		let workspace = doc["workspace"].as_table_mut().unwrap();

		let deps = workspace.get("dependencies").and_then(Item::as_table);
		let found_orig = deps.and_then(|deps| deps.get(dep_name));
		let found_rename = maybe_rename.and_then(|r| deps?.get(r));

		if found_orig.is_some() && found_rename.is_some() {
			log::warn!(
				target: super::super::LOG_TARGET,
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
				target: super::super::LOG_TARGET,
				"Renaming workspace dependency '{dep_name}' to '{rename}'"
			);
			t.insert("package", Value::String(Formatted::new(dep_name.to_string())));
			rename
		} else {
			dep_name
		};

		let new_name = maybe_rename.unwrap_or(name);
		if !workspace.contains_table("dependencies") {
			workspace.insert("dependencies", table());
		}
		let deps = workspace["dependencies"].as_table_mut().unwrap();
		if dep_name != new_name {
			deps.remove(dep_name);
		}
		deps.insert(new_name, Item::Value(Value::InlineTable(t)));

		Ok(())
	}
}
