// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{Findings, MuteSetting, PropagateFeatureCmd};
use crate::{
	autofix::AutoFixer,
	cmd::{lint::error_stats, CommandError, GlobalArgs, RenamedPackage},
	log, CrateId,
};
use cargo_metadata::{Metadata, Package};
use std::{collections::BTreeSet, fs::canonicalize, path::Path};

pub(super) struct Reporter<'a> {
	cmd: &'a PropagateFeatureCmd,
	meta: &'a Metadata,
	allowed_dir: &'a Path,
	feature: &'a str,
	findings: Findings,
	errors: usize,
	fixes: usize,
}

impl<'a> Reporter<'a> {
	pub(super) fn new(
		cmd: &'a PropagateFeatureCmd,
		meta: &'a Metadata,
		allowed_dir: &'a Path,
		feature: &'a str,
		findings: Findings,
	) -> Self {
		Self { cmd, meta, allowed_dir, feature, findings, errors: 0, fixes: 0 }
	}
}

impl Reporter<'_> {
	pub(super) fn report(&mut self, global: &GlobalArgs) -> Result<(), CommandError> {
		let meta = self.meta;
		let ids: BTreeSet<CrateId> = self
			.findings
			.propagate_missing
			.keys()
			.chain(self.findings.feature_missing.keys())
			.chain(self.findings.non_optional_missing.keys())
			.cloned()
			.collect();
		let mut crates = ids
			.into_iter()
			.map(|id| {
				meta.packages
					.iter()
					.find(|pkg| pkg.id.repr == id)
					.ok_or_else(|| format!("Could not find crate {id} in the metadata"))
			})
			.collect::<Result<Vec<_>, _>>()?;
		crates.sort_by(|a, b| a.name.cmp(&b.name));
		for krate in crates {
			self.report_crate(krate)?;
		}
		if let Some(e) = error_stats(self.errors, 0, self.fixes, self.cmd.fixer_args.enable, global)
		{
			println!("{e}");
		}
		if self.errors > self.fixes {
			return Err(CommandError::reported(global.error_code()))
		}
		Ok(())
	}

	fn report_crate(&mut self, krate: &Package) -> Result<(), CommandError> {
		let in_workspace = self.meta.workspace_members.iter().any(|m| m == &krate.id);
		let krate_path = canonicalize(krate.manifest_path.as_std_path())
			.map_err(|e| format!("Could not canonicalize manifest {}: {e}", krate.manifest_path))?;
		let mut fixer = self.fixer(&krate_path)?;
		let mut krate_str = format!("'{}'", krate.name);
		if self.cmd.show_path {
			krate_str.push_str(&format!(" ({})", krate.manifest_path));
		}
		println!("crate {krate_str}\n  feature '{}'", self.feature);

		if !self.report_missing_feature(krate, in_workspace, &mut fixer)? {
			return Ok(())
		}
		self.report_propagation(krate, &mut fixer)?;
		self.report_non_optional(krate, &mut fixer)?;
		if let Some(fixer) = fixer.as_mut() {
			if self.fixes > 0 {
				fixer.save()?;
			}
		}
		Ok(())
	}

	fn fixer(&self, krate_path: &Path) -> Result<Option<AutoFixer>, CommandError> {
		if !self.cmd.fixer_args.enable {
			return Ok(None)
		}
		if krate_path.starts_with(self.allowed_dir) ||
			self.cmd.modify_paths.iter().any(|p| krate_path.starts_with(p))
		{
			return Ok(Some(AutoFixer::from_manifest(krate_path)?))
		}
		log::info!(
			target: super::super::LOG_TARGET,
			"Cargo path is outside of the workspace: {:?} not in {:?}",
			krate_path.display(), self.allowed_dir.display()
		);
		Ok(None)
	}

	fn can_fix(&self, krate: &Package) -> bool {
		self.cmd.fixer_args.enable &&
			self.cmd.fix_package.as_ref().is_none_or(|p| p == &krate.name.to_string())
	}

	// False preserves the original outer-loop continue when a requested fixer is unavailable.
	fn report_missing_feature(
		&mut self,
		krate: &Package,
		in_workspace: bool,
		fixer: &mut Option<AutoFixer>,
	) -> Result<bool, CommandError> {
		let Some(deps) = self.findings.feature_missing.get(&krate.id.to_string()) else {
			return Ok(true)
		};
		println!(
			"    is required by {} dependenc{}:\n      {}",
			deps.len(),
			if deps.len() == 1 { "y" } else { "ies" },
			names(deps),
		);
		if self.can_fix(krate) &&
			self.cmd.left_side_feature_missing == MuteSetting::Fix &&
			(self.cmd.left_side_outside_workspace == MuteSetting::Fix || in_workspace)
		{
			let Some(fixer) = fixer.as_mut() else { return Ok(false) };
			fixer.add_feature(self.feature)?;
			log::info!(target: super::super::LOG_TARGET, "Inserted feature '{}' into '{}'", self.feature, &krate.name);
			self.fixes += 1;
		}
		self.errors += 1;
		Ok(true)
	}

	fn report_propagation(
		&mut self,
		krate: &Package,
		fixer: &mut Option<AutoFixer>,
	) -> Result<(), CommandError> {
		let Some(deps) = self.findings.propagate_missing.get(&krate.id.to_string()) else {
			return Ok(())
		};
		println!("    must propagate to:\n      {}", names(deps));
		if self.can_fix(krate) {
			for dep in deps {
				let dep_name = dep.name();
				if self.cmd.fix_dependency.as_ref().is_some_and(|d| d != &dep_name) {
					continue
				}
				let Some(fixer) = fixer.as_mut() else { continue };
				let non_optional =
					self.cmd.feature_enables_dep.as_ref().is_some_and(|v| {
						v.iter().any(|(f, d)| f == self.feature && d == &dep_name)
					});
				let opt = if !non_optional && dep.optional { "?" } else { "" };
				fixer.add_to_feature(
					self.feature,
					format!("{dep_name}{opt}/{}", self.feature).as_str(),
				)?;
				log::info!(target: super::super::LOG_TARGET, "Inserted '{dep_name}/{}' into '{}'", self.feature, krate.name);
				self.fixes += 1;
			}
		}
		self.errors += deps.len();
		Ok(())
	}

	fn report_non_optional(
		&mut self,
		krate: &Package,
		fixer: &mut Option<AutoFixer>,
	) -> Result<(), CommandError> {
		let Some(deps) = self.findings.non_optional_missing.get(&krate.id.to_string()) else {
			return Ok(())
		};
		println!("    must enable dependency as non-optional:\n      {}", names(deps));
		if self.can_fix(krate) {
			for dep in deps {
				let Some(fixer) = fixer.as_mut() else { continue };
				let name = dep.manifest_name();
				let explicit = format!("dep:{name}");
				// Introducing `dep:name` suppresses Cargo's implicit public `name` feature.
				let activation = match krate.features.get(name).map(Vec::as_slice) {
					Some([only]) if only == &explicit => name,
					_ => &explicit,
				};
				fixer.add_to_feature(self.feature, activation)?;
				log::info!(target: super::super::LOG_TARGET, "Inserted '{activation}' into '{}'", krate.name);
				self.fixes += 1;
			}
		}
		self.errors += deps.len();
		Ok(())
	}
}

fn names(deps: &BTreeSet<RenamedPackage>) -> String {
	let mut named = deps.iter().map(RenamedPackage::display_name).collect::<Vec<_>>();
	named.sort();
	named.join("\n      ")
}
