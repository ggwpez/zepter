// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

mod workspace;

use super::AutoFixer;
use crate::{cmd::transpose::SourceLocationSelector, log};
use cargo_metadata::DependencyKind;
use toml_edit::{DocumentMut, Formatted, InlineTable, Item, Table, Value};

impl AutoFixer {
	pub fn lift_dependency(
		&mut self,
		dname: &str,
		kind: &DependencyKind,
		default_feats: Option<bool>,
		location: &SourceLocationSelector,
	) -> Result<(), String> {
		if let Some(dep) = self.dependency_mut(dname, kind) {
			Self::lift_some_dependency(dep, default_feats, location)?;
		}
		Ok(())
	}

	pub(crate) fn dependency_mut(
		&mut self,
		name: &str,
		kind: &DependencyKind,
	) -> Option<&mut Item> {
		let kind = crate::kind_to_str(kind);
		let doc: &mut DocumentMut = self.doc.as_mut().unwrap();

		if !doc.contains_table(kind) {
			log::warn!(target: super::LOG_TARGET, "No '{}' entry found", kind);
			return None
		}
		let deps: &mut Table = doc[kind].as_table_mut().unwrap();
		deps.get_mut(name)
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
			Self::lift_inline_dependency(as_table, default_feats, location)?;
		} else {
			return Err("Dependency is not a string or an inline table".into())
		}
		Ok(())
	}

	fn lift_inline_dependency(
		as_table: &mut InlineTable,
		default_feats: Option<bool>,
		location: &SourceLocationSelector,
	) -> Result<(), String> {
		if as_table.contains_key("git") {
			return Err("Cannot lift git dependencies".into())
		}

		match location {
			SourceLocationSelector::Remote if as_table.contains_key("path") =>
				return Err("Lifting dependency would change it from a path dependency to a crates-io dependency".into()),
			SourceLocationSelector::Local if as_table.contains_key("version") =>
				return Err("Lifting dependency would change it from a crates-io dependency to a local dependency".into()),
			_ => {},
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
		Ok(())
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
