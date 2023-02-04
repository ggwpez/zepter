use clap::Parser;
use crate::Crate;
use std::collections::BTreeSet;
use std::borrow::Cow;
use crate::dag::Path;

/// Trace the dependency path one crate to another.
#[derive(Debug, Parser)]
pub struct TraceCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	tree_args: super::common::TreeArgs,

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
		let dag = super::common::tree(&self.tree_args);

		let froms = dag.edges.iter().map(|(krate, _)| krate).filter(|krate| krate.name == self.from).collect::<Vec<_>>();
		if froms.is_empty() {
			panic!("Could not find crate {} in the dependency graph", self.from);
		}

		let tos = dag.edges.iter().map(|(krate, _)| krate).filter(|krate| krate.name == self.to).collect::<Vec<_>>();
		if tos.is_empty() {
			panic!("Could not find crate {} in the dependency graph", self.to);
		}

		log::info!("No version or features specified: Checking all {} possibly distinct paths", froms.len() * tos.len());
		let mut paths = BTreeSet::new();
		
		// kartesian product
		for from in froms.iter() {
			for to in tos.iter() {
				match dag.any_path(&from, &to) {
					Some(mut path) => {
						path = path.into_compact();
						self.simplify_path(&mut path);
						paths.insert(path);
					},
					None => {
						continue;
						/*let forward = dag.dag_of(*from.clone());
						let depends = forward.into_transitive_hull_in(&dag);

						if depends.connected(&from, &to) {
							unreachable!(
								"Sanity check failed: {} depends on {} but no path could be found",
								from, to
							);
						}*/
					},
				}
			}
		}
		if paths.is_empty() {
			panic!("No path found");
		}
		log::info!("Found {} distinct paths modulo version and features", paths.len());
		for path in paths {
			println!("{}", path);
		}
	}

	fn simplify_path(&self, path: &mut Path<'_, Crate>) {
		path.0.iter_mut().for_each(|krate| {
			let val = krate.clone().into_owned();

			*krate = Cow::Owned(Crate {
				version: if self.simple { "".into() } else { val.version },
				enabled_features: if self.simple { vec![] } else { val.enabled_features },
				..val
			})
		});
	}
}
