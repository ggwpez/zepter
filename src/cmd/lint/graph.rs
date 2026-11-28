// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{cmd::resolve_dep, dag::Dag};
use cargo_metadata::{Dependency, DependencyKind, Edition, Metadata, Package};
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
		self.1.starts_with("dep:") || self.1.starts_with("#")
	}
}

pub fn build_feature_dag(meta: &Metadata, pkgs: &[Package]) -> Result<FeatureGraph, String> {
	build_feature_dag_with_resolver(meta, pkgs, resolver_one(meta)?)
}

/// Build a graph using an already determined Cargo resolver mode.
pub fn build_feature_dag_with_resolver(
	meta: &Metadata,
	pkgs: &[Package],
	resolver_one: bool,
) -> Result<FeatureGraph, String> {
	let mut graph = FeatureGraph::default();
	for pkg in pkgs {
		graph.add_package(meta, pkg, resolver_one)?;
	}
	Ok(graph)
}

// Cargo takes the resolver from the workspace root, not from individual members.
// A virtual workspace defaults to resolver 1; a root package defaults by edition.
pub(crate) fn resolver_one(meta: &Metadata) -> Result<bool, String> {
	let path = meta.workspace_root.join("Cargo.toml");
	let text = std::fs::read_to_string(&path)
		.map_err(|e| format!("Could not read manifest {path}: {e}"))?;
	let manifest = text
		.parse::<toml_edit::DocumentMut>()
		.map_err(|e| format!("Could not parse manifest {path}: {e}"))?;
	let resolver = manifest
		.get("workspace")
		.and_then(|w| w.get("resolver"))
		.or_else(|| manifest.get("package").and_then(|p| p.get("resolver")));
	if let Some(resolver) = resolver {
		return match resolver.as_str() {
			Some("1") => Ok(true),
			Some("2" | "3") => Ok(false),
			_ => Err(format!("Invalid resolver in manifest {path}")),
		}
	}
	let root = meta.packages.iter().find(|pkg| pkg.manifest_path == path);
	Ok(root.is_none_or(|pkg| matches!(pkg.edition, Edition::E2015 | Edition::E2018)))
}

impl FeatureGraph {
	fn add_package(
		&mut self,
		meta: &Metadata,
		pkg: &Package,
		resolver_one: bool,
	) -> Result<(), String> {
		let mut bindings = BTreeMap::<&str, Vec<(CrateAndFeature, String)>>::new();
		let mut build_bindings = BTreeMap::<&str, Vec<(CrateAndFeature, String)>>::new();
		let mut build_dep_bindings = BTreeMap::<&str, Vec<(CrateAndFeature, String)>>::new();
		let normal_entry = CrateAndFeature::new(&pkg.id.repr, "#entrypoint");
		let build_entry = CrateAndFeature::new(&pkg.id.repr, "#build-entrypoint");
		self.direct.add_node(normal_entry.clone());
		let root_entry = CrateAndFeature::new(&pkg.id.repr, "#root-entrypoint");
		if resolver_one {
			self.direct.add_node(root_entry.clone());
		}
		for dep in &pkg.dependencies {
			if !resolver_one && dep.kind != DependencyKind::Normal {
				continue
			}
			let alias = dep.rename.as_deref().unwrap_or(&dep.name);
			let Some(resolved) = resolve_dep(pkg, dep, meta) else { continue };
			let id = resolved.pkg.id.repr;
			if dep.kind == DependencyKind::Normal {
				let activation = CrateAndFeature::activation(pkg, alias, dep);
				self.add_dependency(&normal_entry, dep, &activation, &id, false);
				bindings.entry(alias).or_default().push((activation, id.clone()));
			}
			if resolver_one && dep.kind != DependencyKind::Development {
				let suffix = dep.target.as_ref().map(|t| format!("@{t}")).unwrap_or_default();
				let activation =
					CrateAndFeature::new(&pkg.id.repr, &format!("#build-dep:{alias}{suffix}"));
				self.add_dependency(&build_entry, dep, &activation, &id, true);
				// Build dependencies of a normally built package also participate.
				if dep.kind == DependencyKind::Build && !dep.optional {
					self.direct.add_edge(normal_entry.clone(), activation.clone());
				}
				if dep.kind == DependencyKind::Build {
					build_dep_bindings
						.entry(alias)
						.or_default()
						.push((activation.clone(), id.clone()));
				}
				build_bindings.entry(alias).or_default().push((activation, id.clone()));
			}
			if resolver_one && dep.kind == DependencyKind::Development {
				// A root dev dependency contributes features without becoming a
				// normal path, including through its own non-dev dependencies.
				let suffix = dep.target.as_ref().map(|t| format!("@{t}")).unwrap_or_default();
				let activation =
					CrateAndFeature::new(&pkg.id.repr, &format!("#dev-dep:{alias}{suffix}"));
				self.add_dependency(&root_entry, dep, &activation, &id, true);
			}
		}
		for (feature, entries) in &pkg.features {
			let source = CrateAndFeature::new(&pkg.id.repr, feature);
			self.direct.add_node(source.clone());
			if resolver_one {
				let build_source =
					CrateAndFeature::new(&pkg.id.repr, &format!("#build-feature:{feature}"));
				self.add_weak(&build_source, &normal_entry, source.clone());
				self.add_weak(&source, &build_entry, build_source.clone());
				self.add_feature_entries(pkg, entries, &build_source, &build_bindings, true, false);
				self.add_feature_entries(pkg, entries, &source, &build_dep_bindings, true, true);
			}
			self.add_feature_entries(pkg, entries, &source, &bindings, false, false);
		}
		Ok(())
	}

	fn add_feature_entries(
		&mut self,
		pkg: &Package,
		entries: &[String],
		source: &CrateAndFeature,
		bindings: &BTreeMap<&str, Vec<(CrateAndFeature, String)>>,
		build: bool,
		bindings_only: bool,
	) {
		for entry in entries {
			if let Some(alias) = entry.strip_prefix("dep:") {
				for (activation, _) in bindings.get(alias).into_iter().flatten() {
					self.direct.add_edge(source.clone(), activation.clone());
				}
			} else if let Some((name, target_feature)) = entry.split_once('/') {
				let alias = name.strip_suffix('?').unwrap_or(name);
				for (activation, id) in bindings.get(alias).into_iter().flatten() {
					let target = Self::feature_node(id, target_feature, build);
					if name == alias {
						self.direct.add_edge(source.clone(), activation.clone());
						self.direct.add_edge(source.clone(), target);
					} else {
						self.add_weak(source, activation, target);
					}
				}
			} else if !bindings_only {
				self.direct
					.add_edge(source.clone(), Self::feature_node(&pkg.id.repr, entry, build));
			}
		}
	}

	fn feature_node(id: &str, feature: &str, build: bool) -> CrateAndFeature {
		if build {
			CrateAndFeature::new(id, &format!("#build-feature:{feature}"))
		} else {
			CrateAndFeature::new(id, feature)
		}
	}

	fn add_dependency(
		&mut self,
		entry: &CrateAndFeature,
		dep: &Dependency,
		activation: &CrateAndFeature,
		id: &str,
		build: bool,
	) {
		if !dep.optional {
			self.direct.add_edge(entry.clone(), activation.clone());
		}
		let target_entry =
			CrateAndFeature::new(id, if build { "#build-entrypoint" } else { "#entrypoint" });
		self.direct.add_edge(activation.clone(), target_entry);
		if dep.uses_default_features {
			self.direct
				.add_edge(activation.clone(), Self::feature_node(id, "default", build));
		}
		for feature in &dep.features {
			self.direct.add_edge(activation.clone(), Self::feature_node(id, feature, build));
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
		let root = CrateAndFeature::new(&from.0, "#root-entrypoint");
		if let Some((root, _)) = self.direct.edges.get_key_value(&root) {
			parents.insert(root, Some(from));
			pending.push(root);
		}
		while let Some(node) = pending.pop() {
			if node != from && matches(node) {
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
