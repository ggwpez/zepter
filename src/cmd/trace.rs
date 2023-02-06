use crate::{dag::Dag, Crate, CrateId};
use cargo_metadata::Metadata;
use clap::Parser;
use std::collections::{BTreeMap, BTreeSet};

/// Trace the dependency path one crate to another.
#[derive(Debug, Parser)]
pub struct TraceCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	tree_args: super::TreeArgs,

	/// Simplified output for easier human understanding.
	#[clap(long, default_value = "false")]
	simple: bool,

	/// The root crate to start from.
	#[clap(index(1))]
	from: String,

	/// The dependency crate to end at.
	#[clap(index(2))]
	to: String,
}

impl TraceCmd {
	pub(crate) fn run(&self) {
		let meta = self.tree_args.load_metadata().expect("Loads metadata");
		let (dag, index) = self.build_dag(meta).expect("Builds dependency graph");
		let lookup = |id: &str| {
			index
				.get(id)
				.unwrap_or_else(|| panic!("Could not find crate {id} in the metadata"))
		};

		let froms = index
			.iter()
			.filter(|(_id, krate)| krate.name == self.from)
			.map(|(id, _)| id)
			.collect::<Vec<_>>();
		if froms.is_empty() {
			panic!("Could not find crate {} in the dependency graph", self.from);
		}

		let tos = index
			.iter()
			.filter(|(_id, krate)| krate.name == self.to)
			.map(|(id, _)| id)
			.collect::<Vec<_>>();
		if tos.is_empty() {
			panic!("Could not find crate {} in the dependency graph", self.to);
		}

		log::info!(
			"No version or features specified: Checking all {} possibly distinct paths",
			froms.len() * tos.len()
		);
		let mut paths = BTreeSet::new();

		for from in froms.iter() {
			for to in tos.iter() {
				if let Some(path) = dag.any_path(from, to) {
					paths.insert(path);
				}
			}
		}
		if paths.is_empty() {
			panic!("No path found");
		}
		log::info!("Found {} distinct paths", paths.len());
		for path in paths {
			// The path uses CrateId, so first translate it to Crates.
			let path = path.translate_owned(|id| lookup(id).clone().strip_version());
			println!("{path}");
		}
	}

	/// Build a dependency graph over the crates ids and return an index of all crates.
	fn build_dag(
		&self,
		meta: Metadata,
	) -> Result<(Dag<CrateId>, BTreeMap<CrateId, Crate>), String> {
		let mut dag = Dag::new();
		let mut index = BTreeMap::new();

		for pkg in meta.packages.into_iter() {
			dag.add_node(pkg.id.to_string());
			index.insert(
				pkg.id.to_string(),
				Crate {
					id: pkg.id.to_string(),
					name: pkg.name,
					version: pkg.version.to_string(),
					features: pkg.features,
				},
			);

			for dep in pkg.dependencies {
				// Resolve dep to its ID, otherwise we dont know which version it is.
				let dep = meta
					.resolve
					.as_ref()
					// TODO horrible code
					.and_then(|resolve| {
						resolve.nodes.iter().find(|node| {
							node.id.to_string().starts_with(format!("{} ", dep.name).as_str())
						})
					});
				if let Some(dep) = dep {
					dag.add_edge(pkg.id.to_string(), dep.id.to_string());
				} else {
					// Can happen for unused deps.
				}
			}
		}

		Ok((dag, index))
	}
}
