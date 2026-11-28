// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{cmd::resolve_dep, dag::Dag};
use cargo_metadata::{Dependency, DependencyKind, Metadata, Package};
use core::fmt::{self, Display, Formatter};
use std::collections::{btree_map::Entry, BTreeMap};

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd, Debug, Hash)]
pub struct CrateAndFeature(pub String, pub String);

impl Display for CrateAndFeature {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}/{}", self.0, self.1)
	}
}

/// Ordinary implications and Cargo's two-input `alias?/feature` rules.
#[derive(Default)]
pub struct FeatureGraph {
	pub direct: Dag<CrateAndFeature>,
	weak: Vec<WeakForward>,
	// Wake a rule when either its declaring feature or its dependency becomes active.
	by_input: BTreeMap<CrateAndFeature, Vec<usize>>,
}

struct WeakForward {
	source: CrateAndFeature,
	activation: CrateAndFeature,
	target: CrateAndFeature,
}

impl CrateAndFeature {
	fn new(pkg: &str, feature: &str) -> Self {
		Self(pkg.into(), feature.into())
	}

	fn activation(pkg: &Package, alias: &str, dep: &Dependency) -> Self {
		let binding = match &dep.target {
			Some(target) => format!("dep:{alias}@{target}"),
			None => format!("dep:{alias}"),
		};
		Self(pkg.id.repr.clone(), binding)
	}

	pub(super) fn internal(&self) -> bool {
		self.1.starts_with("dep:") || self.1 == "#entrypoint"
	}
}

pub fn build_feature_dag(meta: &Metadata, pkgs: &[Package]) -> Result<FeatureGraph, String> {
	let mut graph = FeatureGraph::default();
	for pkg in pkgs {
		graph.add_package(meta, pkg)?;
	}
	Ok(graph)
}

impl FeatureGraph {
	fn add_package(&mut self, meta: &Metadata, pkg: &Package) -> Result<(), String> {
		let mut bindings = BTreeMap::<&str, Vec<(CrateAndFeature, String)>>::new();
		for dep in pkg.dependencies.iter().filter(|d| d.kind == DependencyKind::Normal) {
			let alias = dep.rename.as_deref().unwrap_or(&dep.name);
			// An unresolved declaration (e.g. outside --workspace) has no target in this graph.
			let Some(resolved) = resolve_dep(pkg, dep, meta) else { continue };
			let id = resolved.pkg.id.repr;
			let activation = CrateAndFeature::activation(pkg, alias, dep);
			self.add_dependency(pkg, dep, &activation, &id);
			bindings.entry(alias).or_default().push((activation, id));
		}
		for (feature, entries) in &pkg.features {
			let source = CrateAndFeature::new(&pkg.id.repr, feature);
			for entry in entries {
				if let Some(alias) = entry.strip_prefix("dep:") {
					for (activation, _) in bindings.get(alias).into_iter().flatten() {
						self.direct.add_edge(source.clone(), activation.clone());
					}
				} else if let Some((name, target_feature)) = entry.split_once('/') {
					let alias = name.strip_suffix('?').unwrap_or(name);
					for (activation, id) in bindings.get(alias).into_iter().flatten() {
						let target = CrateAndFeature::new(id, target_feature);
						if name == alias {
							self.direct.add_edge(source.clone(), activation.clone());
							self.direct.add_edge(source.clone(), target);
						} else {
							self.add_weak(&source, activation, target);
						}
					}
				} else {
					self.direct.add_edge(source.clone(), CrateAndFeature::new(&pkg.id.repr, entry));
				}
			}
		}
		Ok(())
	}

	fn add_dependency(
		&mut self,
		pkg: &Package,
		dep: &Dependency,
		activation: &CrateAndFeature,
		id: &str,
	) {
		if !dep.optional {
			self.direct
				.add_edge(CrateAndFeature::new(&pkg.id.repr, "#entrypoint"), activation.clone());
		}
		self.direct
			.add_edge(activation.clone(), CrateAndFeature::new(id, "#entrypoint"));
		if dep.uses_default_features {
			self.direct.add_edge(activation.clone(), CrateAndFeature::new(id, "default"));
		}
		for feature in &dep.features {
			self.direct.add_edge(activation.clone(), CrateAndFeature::new(id, feature));
		}
	}

	fn add_weak(
		&mut self,
		source: &CrateAndFeature,
		activation: &CrateAndFeature,
		target: CrateAndFeature,
	) {
		self.direct.add_node(source.clone());
		let index = self.weak.len();
		self.by_input.entry(source.clone()).or_default().push(index);
		self.by_input.entry(activation.clone()).or_default().push(index);
		self.weak.push(WeakForward {
			source: source.clone(),
			activation: activation.clone(),
			target,
		});
	}

	/// Find a witness under the activation state induced by `from`, not by `--all-features`.
	pub fn implied_path<'a>(
		&'a self,
		from: &'a CrateAndFeature,
		matches: impl Fn(&CrateAndFeature) -> bool,
	) -> Option<Vec<&'a CrateAndFeature>> {
		let mut parents = BTreeMap::from([(from, None)]);
		let mut pending = vec![from];
		let entry = CrateAndFeature::new(&from.0, "#entrypoint");
		if let Some((entry, _)) = self.direct.edges.get_key_value(&entry) {
			if entry != from {
				parents.insert(entry, Some(from));
				pending.push(entry);
			}
		}
		while let Some(node) = pending.pop() {
			if matches(node) {
				let mut path = vec![node];
				let mut current = node;
				while let Some(parent) = parents[current] {
					path.push(parent);
					current = parent;
				}
				path.reverse();
				return Some(path)
			}
			for next in self.direct.edges.get(node).into_iter().flatten() {
				if let Entry::Vacant(e) = parents.entry(next) {
					e.insert(Some(node));
					pending.push(next);
				}
			}
			for &index in self.by_input.get(node).into_iter().flatten() {
				let w = &self.weak[index];
				if parents.contains_key(&w.source) && parents.contains_key(&w.activation) {
					if let Entry::Vacant(e) = parents.entry(&w.target) {
						e.insert(Some(&w.source));
						pending.push(&w.target);
					}
				}
			}
		}
		None
	}
}
