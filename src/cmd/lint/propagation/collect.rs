// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{CrateAndFeature, Dag, Findings, IgnoreSetting, MuteSetting, PropagateFeatureCmd};
use crate::{
	cmd::{resolve_dep, RenamedPackage},
	log,
};
use cargo_metadata::{Metadata, Package};
use std::collections::{BTreeMap, BTreeSet};

struct FeatureCheck<'a> {
	cmd: &'a PropagateFeatureCmd,
	dag: &'a Dag<CrateAndFeature>,
	feature: &'a str,
	ignore_missing: BTreeMap<CrateAndFeature, BTreeSet<CrateAndFeature>>,
}

impl PropagateFeatureCmd {
	pub(super) fn collect_findings(
		&self,
		meta: &Metadata,
		dag: &Dag<CrateAndFeature>,
		feature: &str,
		to_check: &[&Package],
	) -> Result<Findings, String> {
		let check = FeatureCheck {
			cmd: self,
			dag,
			feature,
			ignore_missing: self.ignore_missing_propagate()?,
		};
		let dep_kinds = self.parse_dep_kinds()?;
		let mut findings = Findings::default();

		for &pkg in to_check.iter().filter(|pkg| !self.skip_package(meta, pkg)) {
			let deps = pkg
				.dependencies
				.iter()
				.filter(|dep| dep_kinds.get(&dep.kind) != Some(&IgnoreSetting::Ignore));
			// Unresolved dependencies may be unused or outside the workspace.
			for dep in deps.filter_map(|dep| resolve_dep(pkg, dep, meta)) {
				check.record(&mut findings, pkg, dep);
			}
		}
		Ok(findings)
	}

	fn skip_package(&self, meta: &Metadata, pkg: &Package) -> bool {
		!meta.workspace_members.iter().any(|m| m == &pkg.id) &&
			self.left_side_outside_workspace == MuteSetting::Ignore
	}
}

impl FeatureCheck<'_> {
	fn record(&self, findings: &mut Findings, pkg: &Package, dep: RenamedPackage) {
		let missing = if !dep.pkg.features.contains_key(self.feature) {
			if !self.needs_non_optional(pkg, &dep) {
				return
			}
			&mut findings.non_optional_missing
		} else if !pkg.features.contains_key(self.feature) {
			if self.cmd.left_side_feature_missing == MuteSetting::Ignore {
				return
			}
			&mut findings.feature_missing
		} else {
			if is_propagated(self.dag, pkg, &dep, self.feature) ||
				is_ignored(&self.ignore_missing, pkg, &dep, self.feature)
			{
				return
			}
			&mut findings.propagate_missing
		};
		missing.entry(pkg.id.to_string()).or_default().insert(dep);
	}

	fn needs_non_optional(&self, pkg: &Package, dep: &RenamedPackage) -> bool {
		let Some(requested) = self.cmd.feature_enables_dep.as_ref().filter(|_| dep.optional) else {
			return false
		};
		let name = dep.name();
		let requested = requested.iter().any(|(f, d)| f == self.feature && *d == name);
		requested &&
			!pkg.features.get(self.feature).is_some_and(|features| features.contains(&name))
	}
}

fn is_ignored(
	ignore: &BTreeMap<CrateAndFeature, BTreeSet<CrateAndFeature>>,
	pkg: &Package,
	dep: &RenamedPackage,
	feature: &str,
) -> bool {
	ignore
		.iter()
		.find(|(c, _)| pkg.name == c.0 && c.1 == feature)
		.is_some_and(|(_, lhs)| lhs.iter().any(|i| dep.pkg.name == i.0 && i.1 == feature))
}

fn is_propagated(
	dag: &Dag<CrateAndFeature>,
	pkg: &Package,
	dep: &RenamedPackage,
	feature: &str,
) -> bool {
	let target = CrateAndFeature(dep.pkg.id.repr.clone(), feature.into());
	let want_opt = CrateAndFeature(format!("{}?", &pkg.id), feature.into());
	let want_req = CrateAndFeature(pkg.id.repr.clone(), feature.into());
	if dag.adjacent(&want_opt, &target) || dag.adjacent(&want_req, &target) {
		return true
	}
	let default_entrypoint = CrateAndFeature(pkg.id.repr.clone(), "#entrypoint".into());
	let sub_dag = dag.sub(|CrateAndFeature(p, f)| {
		(p == &pkg.id.repr && f == "#entrypoint") || (p == &dep.pkg.id.repr)
	});
	if let Some(p) = sub_dag.any_path(&default_entrypoint, &target) {
		log::debug!(
			target: super::super::LOG_TARGET,
			"Reachable from the default entrypoint: {:?} vis {:?}",
			target,
			p.0
		);
		return true
	}
	false
}
