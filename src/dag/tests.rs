// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{Dag, Path};
use std::{borrow::Cow, collections::BTreeSet};

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
			dag.edges.get(*k).unwrap(),
			&v.iter().map(|s| (*s).to_string()).collect::<BTreeSet<_>>(),
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

#[test]
fn num_nodes_counts_rhs_only_nodes() {
	// A -> B -> C: C only appears on RHS, but is still a distinct node.
	let dag = dag_from(&[("A", "B"), ("B", "C")]);
	assert_eq!(dag.num_nodes(), 3);
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
	assert_eq!(sub.num_nodes(), 3); // A, B, C
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
	assert_eq!(filtered.num_nodes(), 4); // A, X, C, Y
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
	assert!(parents.contains(&String::from("A")));
	assert!(parents.contains(&String::from("B")));
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

	let all: BTreeSet<_> = dag.node_iter().collect();
	assert_eq!(all.len(), 3); // A, B, C — deduplicated
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
	let values = [(String::from("hello"), 1), (String::from("world"), 2)];
	let path: Path<'_, (String, i32)> = Path(values.iter().map(Cow::Borrowed).collect());
	let translated: Path<'_, String> = path.translate_borrowed(|(s, _)| s);
	assert_eq!(format!("{translated}"), "hello -> world");
}

#[test]
fn path_translate_owned() {
	let path: Path<'_, String> = Path(vec![Cow::Owned("hello".into()), Cow::Owned("world".into())]);
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
