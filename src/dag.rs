#![allow(dead_code)]

use std::{
	borrow::{Cow, ToOwned},
	collections::{BTreeMap, BTreeSet},
};
use core::fmt::{Display, Formatter};

/// Represents *Directed Acyclic Graph* through its edge relation.
///
/// A "node" in that sense is anything on the left- or right-hand side of this relation.
#[derive(Clone)]
pub struct Dag<T> {
	/// Dependant -> Dependency
	/// eg: Polkadot -> Substrate or Me -> Rust
	pub edges: BTreeMap<T, BTreeSet<T>>,
}

impl<T> Default for Dag<T> {
	fn default() -> Self {
		Self { edges: BTreeMap::new() }
	}
}

/// A path inside a DAG.
///
/// The lifetime is the lifetime of the `Dag`s nodes.
#[derive(Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Path<'a, T: ToOwned>(pub Vec<Cow<'a, T>>);

impl<'a> Path<'a, crate::Crate>
{
	pub fn hops(&self) -> usize {
		match self.0.len() {
			0 => unreachable!("Paths cannot be empty"),
			l => l - 1,
		}
	}

	// Compact entries with the same name.
	pub fn into_compact(self) -> Self {
		let mut v = Vec::<crate::Crate>::new();
		for entry in self.0.into_iter().map(|e| e.into_owned()) {
			match v.last_mut() {
				Some(mut last) if last.name == entry.name => {
					if !last.version.is_empty() {
						panic!("Double version");
					}
					last.version = entry.version;
					last.enabled_features.extend(entry.enabled_features.iter().cloned());
				},
				_ => v.push(entry.clone()),
			}
		}
		Self(v.into_iter().map(Cow::Owned).collect())
	}
}

impl<'a, T> TryFrom<Vec<&'a T>> for Path<'a, T>
where
	T: ToOwned,
{
	type Error = ();

	fn try_from(v: Vec<&'a T>) -> Result<Self, Self::Error> {
		if v.is_empty() {
			return Err(())
		}

		Ok(Self(v.into_iter().map(Cow::Borrowed).collect()))
	}
}

impl<T> Display for Path<'_, T>
where
	T: Display + ToOwned,
{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		self.0.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" -> ").fmt(f)
	}
}

impl<T> Dag<T>
where
	T: Ord + PartialEq + Clone,
{
	pub fn add_edge(&mut self, from: T, to: T) {
		self.edges.entry(from).or_default().insert(to);
	}

	pub fn add_node(&mut self, node: T) {
		self.edges.entry(node).or_default();
	}

	/// Whether `from` is directly connected to `to`.
	///
	/// *Directly* means with via an edge.
	pub fn connected(&self, from: &T, to: &T) -> bool {
		self.edges.get(from).map(|v| v.contains(to)).unwrap_or(false)
	}

	/// Whether `from` appears on the lhs of the edge relation.
	///
	/// Aka: Whether `self` has any dependencies.
	pub fn lhs_contains(&self, from: &T) -> bool {
		self.edges.contains_key(from)
	}

	/// Whether `to` appears on the rhs of the edge relation.
	///
	/// Aka: Whether any other node depends on `self`.
	pub fn rhs_contains(&self, to: &T) -> bool {
		self.edges.values().any(|v| v.contains(to))
	}

	/// The `Dag` only containing the node `from` and its direct dependencies.
	///
	/// This can be inflated back to the original `Dag` by calling
	/// `from.into_transitive_hull_in(self)`.
	pub fn dag_of(&self, from: T) -> Self {
		let mut edges = BTreeMap::new();
		let rhs = self.edges.get(&from).cloned().unwrap_or_default();
		edges.insert(from, rhs);
		Self { edges }
	}

	/// Calculate the transitive hull of `self`.
	pub fn transitive_hull(&mut self) {
		let topology = self.clone();
		self.transitive_in(&topology);
	}

	/// Calculate the transitive hull of `self` while using the connectivity of `topology`.
	pub fn transitive_hull_in(&mut self, topology: &Self) {
		self.transitive_in(topology);
	}

	/// Consume `self` and return the transitive hull.
	pub fn into_transitive_hull(mut self) -> Self {
		let topology = self.clone();
		while self.transitive_in(&topology) {}
		self
	}

	/// Consume `self` and return the transitive hull while using the connectivity of `topology`.
	pub fn into_transitive_hull_in(mut self, topology: &Self) -> Self {
		while self.transitive_in(topology) {}
		self
	}

	fn transitive_in(&mut self, topology: &Self) -> bool {
		let mut changed = false;
		// The edges that are added in this stage.
		let mut new_edges: BTreeMap<T, BTreeSet<T>> = BTreeMap::new();

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

	/// Find *a* path from `from` to `to` and return it.
	///
	/// Note that 1) *the* shortest path does not necessarily exist and 2) this function does not
	/// give any guarantee about the returned path.
	pub fn any_path<'a>(&'a self, from: &'a T, to: &T) -> Option<Path<'a, T>> {
		let mut visited = BTreeSet::new();
		let mut stack = vec![(from, vec![from])];

		while let Some((node, mut path)) = stack.pop() {
			if visited.contains(&node) {
				continue
			}
			visited.insert(node);
			if node == to {
				return path.try_into().ok()
			}
			if let Some(neighbors) = self.edges.get(node) {
				for neighbor in neighbors.iter() {
					path.push(neighbor);
					stack.push((neighbor, path.clone()));
					path.pop();
				}
			}
		}
		None
	}

	/// The number of edges in the graph.
	pub fn num_edges(&self) -> usize {
		self.edges.values().map(|v| v.len()).sum()
	}

	pub fn num_nodes(&self) -> usize {
		self.edges.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::*;

	#[rstest]
	#[case(vec![("A", "B"), ("B", "C")], vec![("A", vec!["B", "C"]), ("B", vec!["C"])])]
	#[case(vec![("A", "B"), ("B", "C"), ("C", "D")], vec![("A", vec!["B", "C", "D"]), ("B", vec!["C", "D"]), ("C", vec!["D"])])]
	fn dag_transitive_hull_works(
		#[case] edges: Vec<(&str, &str)>,
		#[case] expected: Vec<(&str, Vec<&str>)>,
	) {
		let mut dag = Dag::<String>::default();
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
}
