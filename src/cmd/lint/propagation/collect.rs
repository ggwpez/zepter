// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{
	CrateAndFeature, FeatureGraph, Findings, IgnoreSetting, MuteSetting, PropagateFeatureCmd,
};
use crate::{
	cmd::{resolve_dep, RenamedPackage},
	log,
};
use cargo_metadata::{Metadata, Package};
use std::collections::{BTreeMap, BTreeSet};

struct FeatureCheck<'a> {
	cmd: &'a PropagateFeatureCmd,
	dag: &'a FeatureGraph,
	feature: &'a str,
	ignore_missing: BTreeMap<CrateAndFeature, BTreeSet<CrateAndFeature>>,
}

impl PropagateFeatureCmd {
	pub(super) fn collect_findings(
		&self,
		meta: &Metadata,
		dag: &FeatureGraph,
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
		let name = dep.manifest_name();
		requested.iter().any(|(f, d)| f == self.feature && d == name) &&
			!feature_activates_dependency(pkg, self.feature, name)
	}
}

fn feature_activates_dependency(pkg: &Package, feature: &str, name: &str) -> bool {
	let mut next = pkg.features.get(feature);
	let mut pending = Vec::new();
	let mut visited = BTreeSet::new();
	while let Some(entries) = next {
		for entry in entries {
			// `name?/feature` only forwards a feature if the dependency is already active.
			let dependency =
				entry.strip_prefix("dep:").or_else(|| entry.split_once('/').map(|(dep, _)| dep));
			match dependency {
				Some(dep) if dep == name => return true,
				None if visited.insert(entry) => pending.extend(pkg.features.get(entry)),
				_ => {},
			}
		}
		next = pending.pop();
	}
	false
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

fn is_propagated(dag: &FeatureGraph, pkg: &Package, dep: &RenamedPackage, feature: &str) -> bool {
	let target = CrateAndFeature(dep.pkg.id.repr.clone(), feature.into());
	let source = CrateAndFeature(pkg.id.repr.clone(), feature.into());
	// Propagation is checked for builds in which this dependency is present, so a
	// weak forward satisfies the lint without implying that it activates the dependency.
	let binding = CrateAndFeature::activation(pkg, dep.manifest_name());
	if dag.direct.adjacent(&source, &target) || dag.forwards_when_active(&source, &binding, &target)
	{
		return true
	}
	// Check what this declaration enables, not what another consumer of the
	// same package might enable in a different build.
	let mut pending: Vec<_> = dag
		.direct
		.edges
		.get(&binding)
		.into_iter()
		.flatten()
		.filter(|node| node.0 == target.0 && !node.internal())
		.collect();
	let mut seen = BTreeSet::new();
	while let Some(node) = pending.pop() {
		if node == &target {
			log::debug!(target: super::super::LOG_TARGET, "Dependency enables {target}");
			return true
		}
		if seen.insert(node) {
			pending.extend(
				dag.direct
					.edges
					.get(node)
					.into_iter()
					.flatten()
					.filter(|next| next.0 == target.0 && !next.internal()),
			);
		}
	}
	false
}
