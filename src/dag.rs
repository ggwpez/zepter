// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Directed Acyclic Graphs ([Dag]) and [Path]s through them.
//!
//! Can be used to build and trace dependencies in a rust workspace.

#![allow(dead_code)]

use core::fmt::{Display, Formatter};
use std::{
	borrow::{Cow, ToOwned},
	collections::{BTreeMap, BTreeSet},
};

/// Represents *Directed Acyclic Graph* through its edge relation.
///
/// A "node" in that sense is anything on the left- or right-hand side of this relation.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Dag<T: Ord> {
	/// Dependant -> Dependency
	/// eg: Polkadot -> Substrate or Me -> Rust
	pub edges: BTreeMap<T, BTreeSet<T>>,
}

impl<T> Display for Dag<T>
where
	T: Display + Ord,
{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		for (from, tos) in self.edges.iter() {
			for to in tos {
				writeln!(f, "{from} -> {to}")?;
			}
		}
		Ok(())
	}
}

impl<T: Ord> Default for Dag<T> {
	fn default() -> Self {
		Self { edges: BTreeMap::new() }
	}
}

/// A path inside a 8Dag].
///
/// Tries to use borrowing when possible to mitigate copy overhead. Paths cannot be empty.
#[derive(Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Path<'a, T: ToOwned>(pub Vec<Cow<'a, T>>);

impl<'a, T> Path<'a, T>
where
	T: ToOwned,
{
	/// The number of hops (edges) in the path.
	///
	/// This is one less than the number of nodes.
	pub fn num_hops(&self) -> usize {
		match self.num_nodes() {
			0 => unreachable!("Paths cannot be empty"),
			l => l - 1,
		}
	}

	/// The number of nodes within the path.
	///
	/// This is one less than the number of edges and never zero.
	pub fn num_nodes(&self) -> usize {
		self.0.len()
	}

	/// Translate self by applying `f` to all hops and borrowing the returned reference.
	pub fn translate_borrowed<'b, F, U>(&'a self, f: F) -> Path<'b, U>
	where
		F: Fn(&'a T) -> &'b U,
		U: ToOwned<Owned = U>,
	{
		Path(self.0.iter().map(|e| Cow::Borrowed(f(e.as_ref()))).collect())
	}

	/// Translate self by applying `f` to all hops and owning the returned value.
	pub fn translate_owned<'b, F, U>(self, f: F) -> Path<'b, U>
	where
		F: Fn(&T) -> U,
		U: ToOwned<Owned = U>,
	{
		Path(self.0.into_iter().map(|e| Cow::Owned(f(e.as_ref()))).collect())
	}

	/// Run `f` on all nodes in the path.
	pub fn for_each<F>(&self, mut f: F)
	where
		F: FnMut(&T),
	{
		for e in self.0.iter() {
			f(e.as_ref());
		}
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
	/// Create a new empty [Dag].
	pub fn new() -> Self {
		Self::default()
	}

	/// Connect two nodes.
	pub fn add_edge(&mut self, from: T, to: T) {
		self.edges.entry(from).or_default().insert(to);
	}

	/// Add a node to the Dag without any edges.
	pub fn add_node(&mut self, node: T) {
		self.edges.entry(node).or_default();
	}

	pub fn degree(&self, node: &T) -> usize {
		self.edges.get(node).map_or(0, |v| v.len())
	}

	/// Whether `from` is directly adjacent to `to`.
	///
	/// *Directly* means with via an edge.
	pub fn adjacent(&self, from: &T, to: &T) -> bool {
		self.edges.get(from).is_some_and(|v| v.contains(to))
	}

	/// Whether `from` is reachable to `to` via.
	pub fn reachable(&self, from: &T, to: &T) -> bool {
		self.any_path(from, to).is_some()
	}

	/// Whether `from` appears on the lhs of the edge relation.
	///
	/// Aka: Whether `self` has any dependencies nodes.
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

	pub fn sub(&self, pred: impl Fn(&T) -> bool) -> Self {
		let mut edges = BTreeMap::new();
		for (k, v) in self.edges.iter() {
			if pred(k) {
				edges.insert(k.clone(), v.clone());
			}
		}
		Self { edges }
	}

	/// Get get a ref to the a LHS node.
	pub fn lhs_node(&self, from: &T) -> Option<&T> {
		self.edges.get_key_value(from).map(|(k, _)| k)
	}

	pub fn lhs_nodes(&self) -> impl Iterator<Item = &T> {
		self.edges.keys()
	}

	pub fn rhs_nodes(&self) -> impl Iterator<Item = &T> {
		self.edges.values().flat_map(|v| v.iter())
	}

	pub fn inverse_lookup<'a>(&'a self, to: &'a T) -> impl Iterator<Item = &'a T> {
		self.edges
			.iter()
			.filter_map(move |(k, v)| if v.contains(to) { Some(k) } else { None })
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

	/// Calculate the transitive hull of `self` by using the connectivity of `topology`.
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

	/// Find *any* path from `from` to `to`.
	///
	/// Note that 1) *the* shortest path does not necessarily exist and 2) this function does not
	/// give any guarantee about the returned path.
	///
	/// This returns `Some` if (and only if) `to` is *reachable* from `from`.
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

	/// Check if any node which fulfills `pred` can be reached from `from` and return the first
	/// path.
	pub fn reachable_predicate<'a>(
		&'a self,
		from: &'a T,
		pred: impl Fn(&T) -> bool,
	) -> Option<Path<'a, T>> {
		let mut visited = BTreeSet::new();
		let mut stack = vec![(from, vec![from])];

		while let Some((node, mut path)) = stack.pop() {
			if visited.contains(&node) {
				continue
			}
			visited.insert(node);
			if pred(node) {
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

	/// The number of nodes in the graph.
	pub fn num_nodes(&self) -> usize {
		self.edges.len()
	}

	pub fn lhs_iter(&self) -> impl Iterator<Item = &T> {
		self.edges.keys()
	}

	pub fn rhs_iter(&self) -> impl Iterator<Item = &T> {
		self.edges.iter().flat_map(|(_, v)| v.iter())
	}

	/// Iterate though all LHS and RHS nodes.
	pub fn node_iter(&self) -> impl Iterator<Item = &T> {
		self.lhs_iter().chain(self.rhs_iter())
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
