// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::NeverImpliesCmd;
use crate::{
	cmd::{
		lint::{build_feature_dag, CrateAndFeature},
		GlobalArgs,
	},
	log,
};
use cargo_metadata::Package;

impl NeverImpliesCmd {
	pub fn run(&self, _global: &GlobalArgs) -> Result<(), String> {
		let meta = self.cargo_args.load_metadata()?;
		log::info!(
			target: super::super::LOG_TARGET,
			"Checking that feature '{}' never implies '{}'",
			self.precondition,
			self.stays_disabled
		);
		let pkgs = &meta.packages;
		let dag = build_feature_dag(&meta, pkgs)?;

		for start in dag.direct.lhs_nodes().filter(|node| !node.internal()) {
			let feature = &start.1;
			if feature != &self.precondition {
				continue
			}
			let Some(path) = dag.implied_path(start, |node| node.1 == self.stays_disabled) else {
				continue
			};
			let out = self.format_path(&path, pkgs)?;
			println!(
				"Feature '{}' implies '{}' via path:\n  {}",
				self.precondition, self.stays_disabled, out
			);
			return Ok(())
		}
		Ok(())
	}

	fn format_path(&self, path: &[&CrateAndFeature], pkgs: &[Package]) -> Result<String, String> {
		let delimiter = self.path_delimiter.replace("\\n", "\n").replace("\\t", "\t");
		let mut out = String::new();
		for (index, CrateAndFeature(id, feature)) in
			path.iter().filter(|node| !node.internal()).enumerate()
		{
			let krate = pkgs
				.iter()
				.find(|pkg| pkg.id.to_string() == *id)
				.ok_or_else(|| format!("Could not find crate '{id}' in the metadata."))?;
			if index != 0 {
				out.push_str(&delimiter);
			}
			out.push_str(&format!("{}/{}", krate.name, feature));
			if self.show_version {
				out.push_str(&format!(" v{}", krate.version));
			}
			if self.show_source {
				match krate.source.as_ref() {
					Some(source) => out.push_str(&format!(" ({})", source.repr)),
					None => out.push_str(" (local)"),
				}
			}
		}
		Ok(out)
	}
}
