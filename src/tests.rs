// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

#![cfg(test)]

use crate::{
	autofix::AutoFixer,
	cmd::fmt::Mode::{self, Canonicalize, Dedub, Sort},
	kind_to_str,
};
use cargo_metadata::DependencyKind::*;
use rstest::*;
use std::{collections::BTreeMap as Map, vec};

#[rstest]
// Keeps comments
#[case(
	r#"[features]
runtime-benchmarks = [
	# TOML comments are preserved
	"sp-runtime/runtime-benchmarks"
]
"#,
	r#"[features]
runtime-benchmarks = [
	# TOML comments are preserved
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
)]
// Keeps newlines
#[case(
	r#"[features]
runtime-benchmarks = [
	
	"sp-runtime/runtime-benchmarks"
]
"#,
	r#"[features]
runtime-benchmarks = [
	
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
)]
// Keeps newlines 2
#[case(
	r#"[features]
runtime-benchmarks = [
	"pallet-balances/runtime-benchmarks",
	
	
	"sp-runtime/runtime-benchmarks"
]
"#,
	r#"[features]
runtime-benchmarks = [
	"pallet-balances/runtime-benchmarks",
	
	
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
)]
// Keeps newlines and comments
#[case(
	r#"
# 1
[features]
# 2
runtime-benchmarks = [
	# 3
	"pallet-balances/runtime-benchmarks",
	# 4
	
	# 5
	"sp-runtime/runtime-benchmarks"
	# 6
]
# 7
"#,
	r#"
# 1
[features]
# 2
runtime-benchmarks = [
	# 3
	"pallet-balances/runtime-benchmarks",
	# 4
	
	# 5
	"sp-runtime/runtime-benchmarks",
	# 6
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
# 7
"#
)]
#[case(
	r#"[features]
runtime-benchmarks = ["sp-runtime/runtime-benchmarks"]
"#,
	r#"[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
)]
#[case(
	r#"[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks"
]
"#,
	r#"[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
)]
#[case(
	r#"[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
]
"#,
	r#"[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
)]
#[case(
	r#"[features]
runtime-benchmarks = []
"#,
	r#"[features]
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
)]
#[case(
	r#"
[package]
name = "something"

[features]
runtime-benchmarks = []
std = ["frame-support/std"]
"#,
	r#"
[package]
name = "something"

[features]
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-support/std",
	"frame-system/std"
]
"#
)]
#[case(
	r#"[features]
runtime-benchmarks = ["sp-runtime/runtime-benchmarks",   "pallet-balances/runtime-benchmarks"]
"#,
	r#"[features]
runtime-benchmarks = [
	"sp-runtime/runtime-benchmarks",
	"pallet-balances/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
std = [
	"frame-system/std"
]
"#
)]
fn add_to_features_works(#[case] before: &str, #[case] after: &str) {
	let mut fixer = AutoFixer::from_raw(before).unwrap();
	fixer
		.add_to_feature("runtime-benchmarks", "frame-support/runtime-benchmarks")
		.unwrap();
	fixer.add_to_feature("std", "frame-system/std").unwrap();
	assert_eq!(fixer.to_string(), after);
}

#[rstest]
#[case(
	r#"[features]
runtime-benchmarks = [
	# Inside empty works
]
"#,
	r#"[features]
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
	# Inside empty works
]
"#
)]
#[case(
	r#"[features]
runtime-benchmarks = [
	# TOML comments are preserved
	"sp-runtime/runtime-benchmarks"
]
"#,
	r#"[features]
runtime-benchmarks = [
	# TOML comments are preserved
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
]
"#
)]
#[case(
	r#"[features]
# TOML comments are preserved
runtime-benchmarks = []
"#,
	r#"[features]
# TOML comments are preserved
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
]
"#
)]
#[case(
	r#"
# First comment
[features]
# Second comment
runtime-benchmarks = []
"#,
	r#"
# First comment
[features]
# Second comment
runtime-benchmarks = [
	"frame-support/runtime-benchmarks"
]
"#
)]
#[case(
	r#"
# First comment
[features]
# Second comment
runtime-benchmarks = [
	# Third comment
	"sp-runtime/runtime-benchmarks",
	# Fourth comment
]
# Fifth comment
"#,
	r#"
# First comment
[features]
# Second comment
runtime-benchmarks = [
	# Third comment
	"sp-runtime/runtime-benchmarks",
	"frame-support/runtime-benchmarks"
	# Fourth comment
]
# Fifth comment
"#
)]
#[case(
	r#"[features]
runtime-benchmarks = [
"B/F0",
"D/F0",
]
"#,
	r#"[features]
runtime-benchmarks = [
	"B/F0",
	"D/F0",
	"frame-support/runtime-benchmarks"
]
"#
)]
fn add_feature_keeps_comments(#[case] before: &str, #[case] after: &str) {
	let mut fixer = AutoFixer::from_raw(before).unwrap();
	fixer
		.add_to_feature("runtime-benchmarks", "frame-support/runtime-benchmarks")
		.unwrap();
	assert_eq!(fixer.to_string(), after);
}

#[test]
fn crate_feature_works_without_section_exists() {
	let before = r#""#;
	let after = r#"[features]
std = [
	"AAA",
	"BBB",
	"CCC"
]
"#;
	let mut fixer = AutoFixer::from_raw(before).unwrap();
	fixer.add_to_feature("std", "AAA").unwrap();
	fixer.add_to_feature("std", "BBB").unwrap();
	fixer.add_to_feature("std", "CCC").unwrap();
	assert_eq!(fixer.to_string(), after);
}

#[test]
fn add_to_feature_keeps_format() {
	let raw = std::fs::read_to_string("Cargo.toml").unwrap();
	let fixer = AutoFixer::from_raw(&raw).unwrap();
	assert_eq!(fixer.to_string(), raw, "Formatting stays");
}

#[rstest]
#[case(r#""#, None)]
// TODO think about trailing newlines
#[case(
	r#"[features]"#,
	Some(
		r#"[features]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
	"A/F0",
	"C/F0",
	"B/F0",
]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
	"A/F0",

	"C/F0",
	"B/F0",
]
G0 = [
	"A/G0",
	"C/G0",
	# hi
	"B/G0",
]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
	"B/F0",

	"C/F0",
]
G0 = [
	"A/G0",
	# hi
	"B/G0",
	"C/G0",
]
"#
	)
)]
fn sort_all_features_works(#[case] input: &str, #[case] modify: Option<&str>) {
	let mut fixer = AutoFixer::from_raw(input).unwrap();
	fixer.sort_all_features().unwrap();
	assert_eq!(fixer.to_string(), modify.unwrap_or(input));
}

#[rstest]
#[case(r#""#, None)]
#[case(
	r#"[features]"#,
	Some(
		r#"[features]
"#
	)
)]
#[case(
	r#"[features]
F0 = ["A/F0"]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
"A/F0"]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
"A/F0"
]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [	"A/F0"	]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [	"A/F0", "B/F0"	]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
		  
  	
	"A/F0",
  
 
	"B/F0"
 	 
]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [	"A/F0",
"B/F0"	]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
    "A/F0",
	"B/F0"	]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = ["A/F0"
	,
	"B/F0"
,
	"C/F0" 
		, ]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
    "A/F0"
  # 1
	,
	"B/F0"
,
	"C/F0" 	,
]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0"
	# 1
	,
	"B/F0",
	"C/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
	
	    # 1    

    "A/F0",
	"B/F0"	]
"#,
	Some(
		r#"[features]
F0 = [
	# 1
	"A/F0",
	"B/F0",
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
	
	    # 1   

    "A/F0",
	
	 # 2 
	
	"B/F0"

	# 3

		]
"#,
	Some(
		r#"[features]
F0 = [
	# 1
	"A/F0",
	# 2
	"B/F0",
	# 3
]
"#
	)
)]
#[case(
	r#"[features]
F0 = [
	
	# 1
		# 2  
# 2  

    "A/F0",
	
	 # 3 
	
	"B/F0"

	# 4

		]
"#,
	Some(
		r#"[features]
F0 = [
	# 1
	# 2
	# 2
	"A/F0",
	# 3
	"B/F0",
	# 4
]
"#
	)
)]
#[case(
	r#"[features]
std = [
        "pallet-election-provider-support-benchmarking?/std",
        "codec/std",
        "scale-info/std",
        "log/std",

        "frame-support/std",
        "frame-system/std",

        "sp-io/std",
        "sp-std/std",
        "sp-core/std",
        "sp-runtime/std",
        "sp-npos-elections/std",
        "sp-arithmetic/std",
        "frame-election-provider-support/std",
        "log/std",

        "frame-benchmarking?/std",
        "rand/std",
        "strum/std",
        "pallet-balances/std",
        "sp-tracing/std"
]"#,
	Some(
		r#"[features]
std = [
	"pallet-election-provider-support-benchmarking?/std",
	"codec/std",
	"scale-info/std",
	"log/std",
	"frame-support/std",
	"frame-system/std",
	"sp-io/std",
	"sp-std/std",
	"sp-core/std",
	"sp-runtime/std",
	"sp-npos-elections/std",
	"sp-arithmetic/std",
	"frame-election-provider-support/std",
	"log/std",
	"frame-benchmarking?/std",
	"rand/std",
	"strum/std",
	"pallet-balances/std",
	"sp-tracing/std",
]
"#
	)
)]
#[case(
	r#"[features]
F0 =  	[  "A/F0"	, 	"B/F0"	]
"#,
	Some(
		r#"[features]
F0 = [
	"A/F0",
	"B/F0",
]
"#
	)
)]
fn format_all_features_works(#[case] input: &str, #[case] modify: Option<&str>) {
	let mut fixer = AutoFixer::from_raw(input).unwrap();
	fixer.format_all_feature(10).unwrap();
	pretty_assertions::assert_str_eq!(fixer.to_string(), modify.unwrap_or(input));
}

#[rstest]
#[case(
	80,
	r#"[features]
F0 = [  "A/F0"	, 	"B/F0"	]
"#,
	Some(
		r#"[features]
F0 = [ "A/F0", "B/F0" ]
"#
	)
)]
#[case(
	30,
	r#"[features]
F0 = [ "LONG/F0", "FEATU/F0" ]
"#,
	Some(
		r#"[features]
F0 = [ "LONG/F0", "FEATU/F0" ]
"#
	)
)]
#[case(
	30,
	r#"[features]
F0 = [ "LONG/F0", "FEATUR/F0" ]
"#,
	Some(
		r#"[features]
F0 = [
	"LONG/F0",
	"FEATUR/F0",
]
"#
	)
)]
#[case(
	30,
	r#"[features]
F0 = [ "LONG/F0", "FEATU/F0" ]
G0 = [ "LONG/F0", "FEATUR/F0" ]
"#,
	Some(
		r#"[features]
F0 = [ "LONG/F0", "FEATU/F0" ]
G0 = [
	"LONG/F0",
	"FEATUR/F0",
]
"#
	)
)]
#[case(
	30,
	r#"[features]
default = [
	"std",
]
"#,
	Some(
		r#"[features]
default = [ "std" ]
"#
	)
)]
// FIXME: known bug
#[case(
	30,
	r#"[features]
default 	=					[ "std" ] # lel
"#,
	Some(
		r#"[features]
default 	=					[ "std" ] # lel
"#
	)
)]
fn format_all_features_line_width_works(
	#[case] line_width: u32,
	#[case] input: &str,
	#[case] modify: Option<&str>,
) {
	let mut fixer = AutoFixer::from_raw(input).unwrap();
	fixer.format_all_feature(line_width).unwrap();
	pretty_assertions::assert_str_eq!(fixer.to_string(), modify.unwrap_or(input));
}

#[rstest]
#[case(vec![], r#""#, None)]
#[case(vec![], r#"
[features]
default = ["std"]
std = ["B", "A"]
"#, Some(r#"
[features]
default = [
	"std",
]
std = [
	"A",
	"B",
]
"#))]
#[case(vec![], r#"
[features]
default = ["std", "std"]
std = ["B", "A", "B", "A"]
"#, Some(r#"
[features]
default = [
	"std",
]
std = [
	"A",
	"B",
]
"#))]
#[case(vec![("std", vec![Sort, Dedub])], r#"
[features]
default = ["std", "std"]
std = ["B", "A", "B", "A"]
"#, Some(r#"
[features]
default = [
	"std",
]
std = [ "A","B"]
"#))]
#[case(vec![("default", vec![Sort, Canonicalize])], r#"
[features]
default = ["std"]
std = ["B", "A"]
"#, Some(r#"
[features]
default = [
	"std",
]
std = [
	"A",
	"B",
]
"#))]
#[case(vec![("default", vec![Sort])], r#"
[features]
default = ["std"]
std = ["B", "A"]
"#, Some(r#"
[features]
default = ["std"]
std = [
	"A",
	"B",
]
"#))]
#[case(vec![("default", vec![Canonicalize])], r#"
[features]
default = ["std"]
std = ["B", "A"]
"#, Some(r#"
[features]
default = [
	"std",
]
std = [
	"A",
	"B",
]
"#))]
#[case(vec![("default", vec![Sort])], r#"
[features]
default = ["std", "A"]
std = ["B", "A"]
"#,
Some(r#"
[features]
default = [ "A","std"]
std = [
	"A",
	"B",
]
"#))]
#[case(vec![("default", vec![Sort])], r#"
[features]
default = ["std", "A"]
std = ["B", "A"]
"#,
Some(r#"
[features]
default = [ "A","std"]
std = [
	"A",
	"B",
]
"#))]
fn canon_some_features_with_modes_works(
	#[case] modes: Vec<(&str, Vec<Mode>)>,
	#[case] input: &str,
	#[case] modify: Option<&str>,
) {
	let mut fixer = AutoFixer::from_raw(input).unwrap();
	let modes = modes.into_iter().map(|(f, m)| (f.into(), m)).collect::<Map<_, _>>();
	fixer.canonicalize_features("krate", &modes, 0).unwrap();
	pretty_assertions::assert_str_eq!(fixer.to_string(), modify.unwrap_or(input));
}

#[rstest]
#[case(
	r#"
[features]
default = [
	"std",
	"A"
]
"#,
	Ok(
		r#"
[features]
default = [ "std", "A" ]
"#
	)
)]
#[case(
	r#"
[features]
default = [
	"std",
	"A",
]
"#,
	Ok(
		r#"
[features]
default = [ "std", "A" ]
"#
	)
)]
#[case(
	r#"
[features]
default = [
	"std",
	"A", # 1
]
"#,
	Err("has trailing")
)]
#[case(
	r#"
[features]
default = [
	"std",
	"A"
	# 1
	,
]
"#,
	Err("has comments")
)]
#[case(
	r#"
[features]
default = [
	# 1
	"std",
	"A",
]
"#,
	Err("has comments")
)]
#[case(
	r#"
[features]
default = [ #1
	"std",
	"A",
]
"#,
	Err("has comments")
)]
#[case(
	r#"
[features]
default = [
	"std",
	"A",
] # 1
"#,
	Ok(
		r#"
[features]
default = [ "std", "A" ] # 1
"#
	)
)]
fn format_feature_oneline_works(#[case] input: &str, #[case] modify: Result<&str, &str>) {
	let mut fixer = AutoFixer::from_raw(input).unwrap();
	let feature = fixer.get_feature_mut("default").unwrap();
	let res = AutoFixer::format_feature_oneline(feature);

	match modify {
		Ok(modify) => {
			pretty_assertions::assert_str_eq!(fixer.to_string(), modify);
			assert!(fixer.modified());
		},
		Err(modify) => {
			assert_eq!(res, Err(modify.into()));
			assert!(!fixer.modified());
		},
	}
}

#[rstest]
#[case(
	r#"[features]
default = [
	"A",
	"std",
]
"#,
	Ok(None)
)]
#[case(
	r#"[features]
default = [
	"std",
	"A",
]
"#,
	Err("Cannot de-duplicate: feature is not sorted: A < std")
)]
#[case(
	r#"[features]
default = [
	"std",
	"A",
	"A?",
]
"#,
	Err("feature 'default': conflicting ? for 'A?'")
)]
#[case(
	r#"[features]
default = [
	"A",
	# Hey
	"A",
]
"#,
	Err("feature 'default': has a comment 'A'")
)]
#[case(
	r#"[features]
default = [
	# Hey
	"A",
	"A",
]
"#,
	Ok(Some(
		r#"[features]
default = [
	# Hey
	"A",
]
"#
	))
)]
#[case(
	r#"[features]
default = [
	"A",
	"A",
	# Hey
]
"#,
	Ok(Some(
		r#"[features]
default = [
	"A",
	# Hey
]
"#
	))
)]
#[case(
	r#"[features]
default = [
	"A",
	"A",
	# Hey
	"std",
]
"#,
	Ok(Some(
		r#"[features]
default = [
	"A",
	# Hey
	"std",
]
"#
	))
)]
fn deduplicate_feature_works(#[case] input: &str, #[case] modify: Result<Option<&str>, &str>) {
	let mut fixer = AutoFixer::from_raw(input).unwrap();
	let feature = fixer.get_feature_mut("default").unwrap();
	let res = AutoFixer::dedub_feature("krate", "default", feature);

	match modify {
		Ok(modify) => {
			res.unwrap();
			pretty_assertions::assert_str_eq!(fixer.to_string(), modify.unwrap_or(input));
		},
		Err(modify) => {
			assert_eq!(res, Err(modify.into()));
		},
	}
}

#[rstest]
#[case("", false, Err("No workspace entry found"))]
// simple
#[case(
	r#"[workspace]"#,
	true,
	Ok(Some(
		r#"[workspace]

[workspace.dependencies]
log = { version = "0.4.20" }
"#
	))
)]
#[case(
	r#"[workspace]"#,
	false,
	Ok(Some(
		r#"[workspace]

[workspace.dependencies]
log = { version = "0.4.20", default-features = false }
"#
	))
)]
// when there is something present already
#[case(
	r#"[workspace]

[workspace.dependencies]
log = { version = "0.4.20" }
"#,
	true,
	Ok(Some(
		r#"[workspace]

[workspace.dependencies]
log = { version = "0.4.20" }
"#
	))
)]
#[case(
	r#"[workspace]

[workspace.dependencies]
log = { default-features = true }
"#,
	true,
	Ok(Some(
		r#"[workspace]

[workspace.dependencies]
log = { default-features = true , version = "0.4.20" }
"#
	))
)]
#[case(
	r#"[workspace]

[workspace.dependencies]
log = { version = "0.4.20" }
"#,
	true,
	Ok(Some(
		r#"[workspace]

[workspace.dependencies]
log = { version = "0.4.20" }
"#
	))
)]
#[case(
	r#"[workspace]

[workspace.dependencies]
log = { version = "0.4.20", default-features = true }
"#, false,
	Err("Dependency 'log' already exists in the workspace with a different 'default-features' fields: 'true' vs 'false'"),
)]
#[case(
	r#"[workspace]

[workspace.dependencies]
log = { version = "0.4.21", default-features = true }
"#, true,
	Err("Dependency 'log' already exists in the workspace with a different 'version' field: '0.4.21' vs '^0.4.20'"),
)]
#[case(
	r#"[workspace]

[workspace.dependencies]
log = { random = "321", version = "0.4.20", hey = true, git = "123" }
"#,
	true,
	Ok(Some(
		r#"[workspace]

[workspace.dependencies]
log = { random = "321", version = "0.4.20", hey = true, git = "123" }
"#
	))
)]
#[case(
	r#"[workspace]

[workspace.dependencies]
log = { random = "321", hey = true, git = "123" }
"#,
	true,
	Ok(Some(
		r#"[workspace]

[workspace.dependencies]
log = { random = "321", hey = true, git = "123" , version = "0.4.20" }
"#
	))
)]
#[case(
	r#"[workspace]

[workspace.dependencies]
log = { random = "321", default-features = true, hey = true, git = "123" }
"#,
	true,
	Ok(Some(
		r#"[workspace]

[workspace.dependencies]
log = { random = "321", default-features = true, hey = true, git = "123" , version = "0.4.20" }
"#
	))
)]
fn inject_workspace_dep_works(
	#[case] input: &str,
	#[case] default: bool,
	#[case] output: Result<Option<&str>, &str>,
) {
	let mut fixer = AutoFixer::from_raw(input).unwrap();
	let res = fixer.add_workspace_dep_inner("log", None, "^0.4.20", default, None);

	match output {
		Ok(modify) => {
			res.unwrap();
			pretty_assertions::assert_str_eq!(modify.unwrap_or(input), fixer.to_string());
		},
		Err(modify) => {
			assert_eq!(res, Err(modify.into()));
		},
	}
}

#[rstest]
#[case(
	r#"[dependencies]
log = { random = "321", default-features = true, hey = true, git = "123" }
"#,
	Some(true),
	Err("'git' or 'path' dependency are currently not supported")
)]
#[case(
	r#"[dependencies]
log = { random = "321", default-features = true, hey = true, path = "123" }
"#,
	Some(true),
	Err("'git' or 'path' dependency are currently not supported")
)]
#[case(
	r#"[dependencies]
log = { random = "321", default-features = true, version = "0.4.20", hey = true }
"#,
	Some(true),
	Ok(Some(
		r#"[dependencies]
log = { random = "321", default-features = true, hey = true , workspace = true }
"#
	))
)]
#[case(
	r#"[dependencies]
log = "321"
"#,
	Some(true),
	Ok(Some(
		r#"[dependencies]
log = { workspace = true, default-features = true }
"#
	))
)]
#[case(
	r#"[dependencies]
log = "321"
"#,
	Some(false),
	Ok(Some(
		r#"[dependencies]
log = { workspace = true, default-features = false }
"#
	))
)]
#[case(
	r#"[dependencies]
log = "321"
"#,
	None,
	Ok(Some(
		r#"[dependencies]
log = { workspace = true }
"#
	))
)]
fn lift_to_workspace_works(
	#[case] input: &str,
	#[case] default: Option<bool>,
	#[case] output: Result<Option<&str>, &str>,
) {
	for kind in [Normal, Build, Development] {
		let table = kind_to_str(&kind);
		let input = &input.replace("dependencies", table);
		let mut fixer = AutoFixer::from_raw(input).unwrap();
		let res = fixer.lift_dependency("log", &kind, default, &crate::cmd::transpose::SourceLocationSelector::Remote);

		match output {
			Ok(modify) => {
				res.unwrap();
				let modify = modify.unwrap_or(input).replace("dependencies", table);
				pretty_assertions::assert_str_eq!(fixer.to_string(), modify);
			},
			Err(modify) => {
				assert_eq!(res, Err(modify.into()));
			},
		}
	}
}
