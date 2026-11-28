// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{cmd::resolve_dep, dag::Dag};
use cargo_metadata::{Metadata, Package};
use core::fmt::{self, Display, Formatter};

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd, Debug, Hash)]
pub struct CrateAndFeature(pub String, pub String);

impl Display for CrateAndFeature {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}/{}", self.0, self.1)
	}
}

// Complexity is `O(x ^ 4) with x=pkgs.len()`.
pub fn build_feature_dag(
	meta: &Metadata,
	pkgs: &[Package],
) -> Result<Dag<CrateAndFeature>, String> {
	let mut dag = Dag::new();
	for pkg in pkgs.iter() {
		add_dependency_edges(&mut dag, meta, pkg);
		add_feature_edges(&mut dag, meta, pkg)?;
	}
	Ok(dag)
}

fn add_dependency_edges(dag: &mut Dag<CrateAndFeature>, meta: &Metadata, pkg: &Package) {
	for dep in &pkg.dependencies {
		if dep.uses_default_features {
			dag.add_edge(
				CrateAndFeature(pkg.id.to_string(), "default".into()),
				CrateAndFeature(dep.name.clone(), "default".into()),
			);

			let Some(dep_id) = resolve_dep(pkg, dep, meta) else { continue };

			// Hacky…
			dag.add_edge(
				CrateAndFeature(pkg.id.to_string(), "#entrypoint".into()),
				CrateAndFeature(dep_id.pkg.id.repr.clone(), "default".into()),
			);
		}
		for feature in &dep.features {
			dag.add_edge(
				CrateAndFeature(pkg.id.to_string(), "default".into()),
				CrateAndFeature(dep.name.clone(), feature.into()),
			);
		}
	}
}

fn add_feature_edges(
	dag: &mut Dag<CrateAndFeature>,
	meta: &Metadata,
	pkg: &Package,
) -> Result<(), String> {
	for (feature, deps) in pkg.features.iter() {
		for dep in deps {
			if dep.contains(':') {
				let (_, dep) = dep
					.split_once(':')
					.ok_or_else(|| format!("Invalid dependency feature '{dep}' in {}", pkg.id))?;
				let dep_feature = "default";
				let dep = pkg
					.dependencies
					.iter()
					.find(|d| d.rename.as_ref().unwrap_or(&d.name) == dep)
					.ok_or_else(|| format!("Could not resolve dep {dep} of {}", pkg.id))?;

				let dep_id = match resolve_dep(pkg, dep, meta) {
					None => {
						// This can happen for optional dependencies who are not enabled, or
						// a weird `target` is specified or it is a dev dependency.
						// In this case we just go by name. It is a dead-end anyway.
						dep.name.clone()
					},
					Some(dep) => dep.pkg.id.to_string(), // TODO rename
				};
				dag.add_edge(
					CrateAndFeature(pkg.id.to_string(), feature.clone()),
					CrateAndFeature(dep_id.clone(), dep_feature.into()),
				);
			} else if dep.contains('/') {
				let (dep, dep_feature) = dep
					.split_once('/')
					.ok_or_else(|| format!("Invalid dependency feature '{dep}' in {}", pkg.id))?;
				let dep = dep.replace('?', "");

				let dep = pkg
					.dependencies
					.iter()
					.find(|d| d.rename.as_ref().unwrap_or(&d.name) == &dep)
					.ok_or_else(|| format!("Could not resolve dep {} of {}", dep, pkg.id))?;

				let dep_id = match resolve_dep(pkg, dep, meta) {
					None => {
						// This can happen for optional dependencies who are not enabled, or
						// a weird `target` is specified or it is a dev dependency.
						// In this case we just go by name. It is a dead-end anyway.
						dep.name.clone()
					},
					Some(dep) => dep.pkg.id.to_string(), // TODO rename
				};
				dag.add_edge(
					CrateAndFeature(pkg.id.to_string(), feature.clone()),
					CrateAndFeature(dep_id.clone(), dep_feature.into()),
				);
			} else {
				let dep_feature = dep;
				// Enables one of its own features.
				dag.add_edge(
					CrateAndFeature(pkg.id.to_string(), feature.clone()),
					CrateAndFeature(pkg.id.to_string(), dep_feature.into()),
				);
			}
		}
	}
	Ok(())
}
