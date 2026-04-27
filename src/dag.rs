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
		while self.transitive_in(&topology) {}
	}

	/// Calculate the transitive hull of `self` while using the connectivity of `topology`.
	pub fn transitive_hull_in(&mut self, topology: &Self) {
		while self.transitive_in(topology) {}
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

	/// Helper to build a Dag from edge pairs.
	fn dag_from(edges: &[(&str, &str)]) -> Dag<String> {
		let mut dag = Dag::new();
		for (from, to) in edges {
			dag.add_edge((*from).into(), (*to).into());
		}
		dag
	}

	/// Helper to assert edges of a Dag.
	fn assert_edges(dag: &Dag<String>, expected: &[(&str, &[&str])]) {
		for (k, v) in expected {
			assert_eq!(
				dag.edges.get(&k.to_string()).unwrap(),
				&v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(),
				"edges for node '{k}' don't match"
			);
		}
	}

	/// The transitive closure of A→B→C→D must contain edge A→D.
	#[test]
	fn transitive_hull_is_complete() {
		let mut dag = dag_from(&[("A", "B"), ("B", "C"), ("C", "D")]);
		dag.transitive_hull();
		assert_edges(&dag, &[("A", &["B", "C", "D"]), ("B", &["C", "D"]), ("C", &["D"])]);
		// Idempotency: running again should not change anything.
		let before = dag.num_edges();
		dag.transitive_hull();
		assert_eq!(dag.num_edges(), before);
	}

	// === num_nodes undercounts ===

	#[test]
	fn num_nodes_misses_rhs_only_nodes() {
		// A -> B -> C: C only appears on RHS
		let dag = dag_from(&[("A", "B"), ("B", "C")]);
		// There are 3 distinct nodes: A, B, C
		// But num_nodes() only counts LHS keys = {A, B} = 2
		assert_eq!(dag.num_nodes(), 2, "BUG: num_nodes only counts LHS, misses leaf node C");
	}

	// === add_edge / add_node / degree ===

	#[test]
	fn add_node_creates_empty_entry() {
		let mut dag = Dag::<String>::new();
		dag.add_node("X".into());
		assert!(dag.lhs_contains(&"X".into()));
		assert_eq!(dag.degree(&"X".into()), 0);
		assert_eq!(dag.num_edges(), 0);
	}

	#[test]
	fn add_edge_duplicate_is_idempotent() {
		let mut dag = Dag::<String>::new();
		dag.add_edge("A".into(), "B".into());
		dag.add_edge("A".into(), "B".into());
		assert_eq!(dag.degree(&"A".into()), 1);
		assert_eq!(dag.num_edges(), 1);
	}

	#[test]
	fn degree_of_missing_node_is_zero() {
		let dag = Dag::<String>::new();
		assert_eq!(dag.degree(&"X".into()), 0);
	}

	// === adjacent ===

	#[test]
	fn adjacent_direct_edge() {
		let dag = dag_from(&[("A", "B"), ("B", "C")]);
		assert!(dag.adjacent(&"A".into(), &"B".into()));
		assert!(!dag.adjacent(&"A".into(), &"C".into()), "not directly adjacent");
		assert!(!dag.adjacent(&"B".into(), &"A".into()), "wrong direction");
	}

	// === reachable ===

	#[test]
	fn reachable_through_chain() {
		let dag = dag_from(&[("A", "B"), ("B", "C"), ("C", "D")]);
		assert!(dag.reachable(&"A".into(), &"D".into()));
		assert!(dag.reachable(&"A".into(), &"B".into()));
		assert!(!dag.reachable(&"D".into(), &"A".into()), "not reachable backwards");
	}

	#[test]
	fn reachable_disconnected() {
		let dag = dag_from(&[("A", "B"), ("C", "D")]);
		assert!(!dag.reachable(&"A".into(), &"D".into()));
		assert!(!dag.reachable(&"C".into(), &"B".into()));
	}

	// === any_path ===

	#[test]
	fn any_path_returns_valid_path() {
		let dag = dag_from(&[("A", "B"), ("B", "C"), ("C", "D")]);
		let (a, d) = (String::from("A"), String::from("D"));
		let path = dag.any_path(&a, &d).unwrap();
		assert_eq!(path.num_hops(), 3);
		assert_eq!(path.0[0].as_ref(), "A");
		assert_eq!(path.0[3].as_ref(), "D");
	}

	#[test]
	fn any_path_from_equals_to() {
		let dag = dag_from(&[("A", "B")]);
		let a = String::from("A");
		let path = dag.any_path(&a, &a).unwrap();
		assert_eq!(path.num_nodes(), 1);
		assert_eq!(path.num_hops(), 0);
	}

	#[test]
	fn any_path_not_found() {
		let dag = dag_from(&[("A", "B")]);
		assert!(dag.any_path(&"B".into(), &"A".into()).is_none());
	}

	#[test]
	fn any_path_node_not_in_graph() {
		let dag = dag_from(&[("A", "B")]);
		assert!(dag.any_path(&"X".into(), &"Y".into()).is_none());
	}

	#[test]
	fn any_path_handles_cycle() {
		let dag = dag_from(&[("A", "B"), ("B", "C"), ("C", "A")]);
		// Despite the cycle, it should find a path and not loop forever.
		let (a, c) = (String::from("A"), String::from("C"));
		let path = dag.any_path(&a, &c).unwrap();
		assert!(path.num_hops() >= 1);
		// And it should not find a path to a node that's not in the graph.
		assert!(dag.any_path(&a, &"X".into()).is_none());
	}

	// === reachable_predicate ===

	#[test]
	fn reachable_predicate_finds_target() {
		let dag = dag_from(&[("A", "B"), ("B", "C"), ("C", "D")]);
		let a = String::from("A");
		let path = dag.reachable_predicate(&a, |n| n == "D").unwrap();
		assert_eq!(path.0.last().unwrap().as_ref(), "D");
	}

	#[test]
	fn reachable_predicate_matches_start() {
		let dag = dag_from(&[("A", "B")]);
		let a = String::from("A");
		let path = dag.reachable_predicate(&a, |n| n == "A").unwrap();
		assert_eq!(path.num_nodes(), 1);
	}

	#[test]
	fn reachable_predicate_no_match() {
		let dag = dag_from(&[("A", "B"), ("B", "C")]);
		let a = String::from("A");
		assert!(dag.reachable_predicate(&a, |n| n == "Z").is_none());
	}

	// === lhs_contains / rhs_contains ===

	#[test]
	fn lhs_rhs_contains() {
		let dag = dag_from(&[("A", "B"), ("B", "C")]);
		assert!(dag.lhs_contains(&"A".into()));
		assert!(dag.lhs_contains(&"B".into()));
		assert!(!dag.lhs_contains(&"C".into()), "C is only on RHS");

		assert!(!dag.rhs_contains(&"A".into()), "A is only on LHS");
		assert!(dag.rhs_contains(&"B".into()));
		assert!(dag.rhs_contains(&"C".into()));
	}

	// === dag_of ===

	#[test]
	fn dag_of_returns_single_node_subgraph() {
		let dag = dag_from(&[("A", "B"), ("A", "C"), ("B", "D")]);
		let sub = dag.dag_of("A".into());
		assert_eq!(sub.num_nodes(), 1);
		assert_eq!(sub.num_edges(), 2);
		assert!(sub.adjacent(&"A".into(), &"B".into()));
		assert!(sub.adjacent(&"A".into(), &"C".into()));
	}

	#[test]
	fn dag_of_missing_node() {
		let dag = dag_from(&[("A", "B")]);
		let sub = dag.dag_of("X".into());
		assert_eq!(sub.num_nodes(), 1);
		assert_eq!(sub.num_edges(), 0);
	}

	// === sub ===

	#[test]
	fn sub_filters_lhs_by_predicate() {
		let dag = dag_from(&[("A", "X"), ("B", "X"), ("C", "Y")]);
		let filtered = dag.sub(|n| n == "A" || n == "C");
		assert_eq!(filtered.num_nodes(), 2);
		assert!(filtered.lhs_contains(&"A".into()));
		assert!(!filtered.lhs_contains(&"B".into()));
		assert!(filtered.lhs_contains(&"C".into()));
	}

	// === inverse_lookup ===

	#[test]
	fn inverse_lookup_finds_parents() {
		let dag = dag_from(&[("A", "C"), ("B", "C"), ("D", "E")]);
		let c = String::from("C");
		let parents: BTreeSet<_> = dag.inverse_lookup(&c).collect();
		assert_eq!(parents.len(), 2);
		assert!(parents.contains(&"A".to_string()));
		assert!(parents.contains(&"B".to_string()));
	}

	#[test]
	fn inverse_lookup_no_parents() {
		let dag = dag_from(&[("A", "B")]);
		let a = String::from("A");
		let parents: Vec<_> = dag.inverse_lookup(&a).collect();
		assert!(parents.is_empty());
	}

	// === num_edges ===

	#[test]
	fn num_edges_counts_correctly() {
		let dag = dag_from(&[("A", "B"), ("A", "C"), ("B", "C")]);
		assert_eq!(dag.num_edges(), 3);
	}

	#[test]
	fn empty_dag_counts() {
		let dag = Dag::<String>::new();
		assert_eq!(dag.num_edges(), 0);
		assert_eq!(dag.num_nodes(), 0);
	}

	// === lhs_iter / rhs_iter / node_iter ===

	#[test]
	fn iterators_cover_all_sides() {
		let dag = dag_from(&[("A", "B"), ("B", "C")]);
		let lhs: BTreeSet<_> = dag.lhs_iter().collect();
		assert_eq!(lhs.len(), 2); // A, B

		let rhs: Vec<_> = dag.rhs_iter().collect();
		assert_eq!(rhs.len(), 2); // B, C (B appears as both lhs and rhs)

		let all: Vec<_> = dag.node_iter().collect();
		assert_eq!(all.len(), 4); // A, B, B, C (not deduplicated)
	}

	// === Display ===

	#[test]
	fn dag_display() {
		let dag = dag_from(&[("A", "B")]);
		let s = format!("{dag}");
		assert_eq!(s, "A -> B\n");
	}

	// === Path ===

	#[test]
	fn path_try_from_empty_fails() {
		let empty: Vec<&String> = vec![];
		assert!(Path::try_from(empty).is_err());
	}

	#[test]
	fn path_display() {
		let path: Path<'_, String> =
			Path(vec![Cow::Owned("A".into()), Cow::Owned("B".into()), Cow::Owned("C".into())]);
		assert_eq!(format!("{path}"), "A -> B -> C");
	}

	#[test]
	fn path_for_each() {
		let path: Path<'_, String> = Path(vec![Cow::Owned("A".into()), Cow::Owned("B".into())]);
		let mut visited = vec![];
		path.for_each(|n| visited.push(n.clone()));
		assert_eq!(visited, vec!["A", "B"]);
	}

	#[test]
	fn path_translate_borrowed() {
		let values = vec![(String::from("hello"), 1), (String::from("world"), 2)];
		let path: Path<'_, (String, i32)> = Path(values.iter().map(|v| Cow::Borrowed(v)).collect());
		let translated: Path<'_, String> = path.translate_borrowed(|&(ref s, _)| s);
		assert_eq!(format!("{translated}"), "hello -> world");
	}

	#[test]
	fn path_translate_owned() {
		let path: Path<'_, String> =
			Path(vec![Cow::Owned("hello".into()), Cow::Owned("world".into())]);
		let upper: Path<'_, String> = path.translate_owned(|s| s.to_uppercase());
		assert_eq!(format!("{upper}"), "HELLO -> WORLD");
	}

	// === into_transitive_hull with diamond ===

	#[test]
	fn transitive_hull_diamond() {
		//   A
		//  / \
		// B   C
		//  \ /
		//   D
		let dag = dag_from(&[("A", "B"), ("A", "C"), ("B", "D"), ("C", "D")]);
		let dag = dag.into_transitive_hull();
		assert_edges(&dag, &[("A", &["B", "C", "D"]), ("B", &["D"]), ("C", &["D"])]);
	}

	#[test]
	fn transitive_hull_wide_fan() {
		// A -> B, A -> C, A -> D (no chaining, hull should be same)
		let dag = dag_from(&[("A", "B"), ("A", "C"), ("A", "D")]);
		let before = dag.num_edges();
		let dag = dag.into_transitive_hull();
		assert_eq!(dag.num_edges(), before);
	}

	// === into_transitive_hull_in with partial view ===

	#[test]
	fn into_transitive_hull_in_expands_partial_view() {
		let topology = dag_from(&[("A", "B"), ("B", "C"), ("B", "D"), ("C", "E")]);
		// Start with just A->B, expand using full topology
		let dag = dag_from(&[("A", "B")]);
		let dag = dag.into_transitive_hull_in(&topology);

		let a_deps = dag.edges.get("A").unwrap();
		assert!(a_deps.contains("B"));
		assert!(a_deps.contains("C"));
		assert!(a_deps.contains("D"));
		assert!(a_deps.contains("E"));
	}
}
