use cargo_metadata::{CargoOpt, *};
use clap::Parser;
use env_logger::Env;
use std::{
	collections::{BTreeMap, BTreeSet},
	path::PathBuf,
};

// Just store the edges
#[derive(Default, Clone)]
pub struct DAG {
	// Dependant -> Dependency
	// eg: Substrate -> Polkadot and Polkadot -> Cumulus
	pub edges: BTreeMap<String, BTreeSet<String>>,
}

impl DAG {
	pub fn add_edge(&mut self, from: String, to: String) {
		self.edges.entry(from.clone()).or_default().insert(to.clone());
	}

	pub fn connected(&self, from: &str, to: &str) -> bool {
		self.edges.get(from).map(|v| v.contains(to)).unwrap_or(false)
	}

	pub fn contains(&self, from: &str) -> bool {
		self.edges.contains_key(from)
	}

	pub fn dag_of(&self, from: &str) -> Self {
		let mut edges = BTreeMap::new();
		edges.insert(from.to_string(), self.edges.get(from).map(|s| s.clone()).unwrap_or_default());
		Self { edges }
	}

	fn transitive_in(&mut self, topology: &DAG) -> bool {
		let mut changed = false;
		// The edges that are added in this stage.
		let mut new_edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

		for (k, vs) in self.edges.iter() {
			for v in vs {
				if let Some(new_deps) = topology.edges.get(v) {
					let edge = self.edges.get(k);
					for new_dep in new_deps {
						if !edge.unwrap().contains(new_dep) {
							new_edges.entry(k.clone()).or_default().insert(new_dep.clone());
							changed = true;
						}
					}
				}
			}
		}

		for (k, v) in new_edges {
			self.edges.entry(k).or_default().extend(v);
		}

		changed
	}

	pub fn transitive_hull(&mut self) {
		let topology = self.clone();
		self.transitive_in(&topology);
	}

	pub fn transitive_hull_in(&mut self, topology: &Self) {
		self.transitive_in(topology);
	}

	pub fn into_transitive_hull(mut self) -> Self {
		let topology = self.clone();
		while self.transitive_in(&topology) {}
		self
	}

	pub fn into_transitive_hull_in(mut self, topology: &Self) -> Self {
		while self.transitive_in(topology) {}
		self
	}

	pub fn into_inverted(mut self) -> Self {
		let mut new_edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
		for (k, v) in self.edges.iter() {
			for dep in v {
				new_edges.entry(dep.clone()).or_default().insert(k.clone());
			}
		}
		Self { edges: new_edges }
	}

	/// Find a path from `from` to `to` and return it.
	pub fn path(&self, from: &str, to: &str) -> Option<Vec<String>> {
		let mut visited = BTreeSet::new();
		let mut stack = vec![(from.to_string(), vec![from.to_string()])];
		while let Some((node, mut path)) = stack.pop() {
			if visited.contains(&node) {
				continue
			}
			visited.insert(node.clone());
			if node == to {
				return Some(path)
			}
			if let Some(neighbors) = self.edges.get(&node) {
				for neighbor in neighbors {
					path.push(neighbor.clone());
					stack.push((neighbor.clone(), path.clone()));
					path.pop();
				}
			}
		}
		None
	}

	/// Find all paths from `from` to `to` and return them, if any.
	pub fn all_paths(&self, from: &str, to: &str) -> Vec<Vec<String>> {
		let mut paths: Vec<Vec<String>> = vec![];
		let mut path = vec![];
		Self::dfs(&self.edges, from, to, path, &mut paths);
		paths
	}

	fn dfs(
		edges: &BTreeMap<String, BTreeSet<String>>,
		from: &str,
		to: &str,
		mut path: Vec<String>,
		paths: &mut Vec<Vec<String>>,
	) {
		if from == to {
			paths.push(path);
		} else {
			for neighbor in edges.get(from).unwrap_or(&BTreeSet::new()) {
				path.push(neighbor.clone());
				let mut paths = Self::dfs(edges, neighbor, to, path.clone(), paths);
				path.pop();
			}
		}
	}

	/// Same as above but using a stack instead of recursion.
	pub fn all_paths_fast(&self, from: &str, to: &str) -> Vec<Vec<String>> {
		let mut paths: Vec<Vec<String>> = vec![];
		let mut stack = vec![(from.to_string(), vec![from.to_string()])];
		while let Some((node, mut path)) = stack.pop() {
			if node == to {
				paths.push(path.clone());
			}
			if let Some(neighbors) = self.edges.get(&node) {
				for neighbor in neighbors {
					path.push(neighbor.clone());
					stack.push((neighbor.clone(), path.clone()));
					path.pop();
				}
			}
		}
		paths
	}

	pub fn num_edges(&self) -> usize {
		self.edges.values().map(|v| v.len()).sum()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::*;

	#[test]
	fn rayondsf() {
		use rayon::prelude::*;
		let test = BTreeSet::<String>::new();
		test.par_iter();

		let map = BTreeMap::<String, BTreeSet<String>>::new();
		map.par_iter();
	}

	#[rstest]
	#[case(vec![("A", "B"), ("B", "C")], vec![("A", vec!["B", "C"]), ("B", vec!["C"])])]
	#[case(vec![("A", "B"), ("B", "C"), ("C", "D")], vec![("A", vec!["B", "C", "D"]), ("B", vec!["C", "D"]), ("C", vec!["D"])])]
	fn dag_transitive_hull_works(
		#[case] edges: Vec<(&str, &str)>,
		#[case] expected: Vec<(&str, Vec<&str>)>,
	) {
		let mut dag = DAG::default();
		for (from, to) in edges {
			dag.add_edge(from.into(), to.into());
		}
		let dag = dag.into_transitive_hull();
		for (k, v) in expected {
			assert_eq!(
				dag.edges.get(k).unwrap(),
				&v.into_iter().map(|s| s.into()).collect::<BTreeSet<_>>()
			);
		}
		let dag2 = dag.clone().into_transitive_hull();
		assert_eq!(dag.num_edges(), dag2.num_edges());
	}

	/*#[rstest]
	#[case(vec![("A", "B"), ("B", "C")], vec![("C", vec!["B"]), ("B", vec!["A"])])]
	fn dag_invert_works(
		#[case] edges: Vec<(&str, &str)>,
		#[case] expected: Vec<(&str, Vec<&str>)>,
	) {
		let mut dag = DAG::default();
		for (from, to) in edges {
			dag.add_edge(from.into(), to.into());
		}
		let dag = dag.invert();
		for (k, v) in expected {
			assert_eq!(
				dag.edges.get(k).unwrap(),
				&v.into_iter().map(|s| s.into()).collect::<BTreeSet<_>>()
			);
		}
	}*/
}
