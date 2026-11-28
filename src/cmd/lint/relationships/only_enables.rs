// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::OnlyEnablesCmd;
use crate::{
	cmd::{resolve_dep, GlobalArgs, RenamedPackage},
	log,
};
use cargo_metadata::Package;

impl OnlyEnablesCmd {
	pub fn run(&self, _global: &GlobalArgs) -> Result<(), String> {
		let meta = self.cargo_args.load_metadata()?;
		for pkg in &meta.packages {
			for dep in &pkg.dependencies {
				let Some(dep) = resolve_dep(pkg, dep, &meta) else { continue };
				if dep.pkg.features.contains_key(&self.only_enables) {
					self.report_enabling_features(pkg, &dep);
				}
			}
		}
		Ok(())
	}

	fn report_enabling_features(&self, pkg: &Package, dep: &RenamedPackage) {
		for (feat, imply) in &pkg.features {
			if feat == &self.precondition {
				continue
			}
			log::info!(target: super::super::LOG_TARGET, "{}: {}", feat, imply.join(", "));

			let opt = if dep.optional { "?" } else { "" };
			let bad_opt = format!("{}{}/{}", dep.name(), opt, self.only_enables);
			let bad = format!("{}/{}", dep.name(), self.only_enables);
			if imply.contains(&bad) || imply.contains(&bad_opt) {
				println!("{}/{} enables {}/{}", pkg.name, feat, dep.name(), self.only_enables);
			}
		}
	}
}
