// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

#![cfg(test)]

use crate::{
	autofix::AutoFixer,
	cmd::fmt::{
		Mode,
		Mode::{Canonicalize, Sort},
	},
};
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
#[case(r#""#, true)]
#[case(r#"[features]"#, true)]
#[case(
	r#"[features]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]"#,
	true
)]
#[case(
	r#"[features]
F0 = [
"B/F0",
"A/F0",
]"#,
	false
)]
#[case(
	r#"[features]
G0 = [
	"B/F0",
	"A/F0",
]"#,
	true
)]
#[case(
	r#"[features]
G0 = [
	"B/F0",
	"A/F0",
]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]"#,
	true
)]
#[case(
	r#"[features]
G0 = [
	"B/F0",
	"A/F0",
]
F0 = [
"B/F0",
"A/F0",
]"#,
	false
)]
fn check_sorted_feature_works(#[case] input: &str, #[case] good: bool) {
	let fixer = AutoFixer::from_raw(input).unwrap();
	assert_eq!(fixer.check_sorted_feature("F0"), good);
}

#[rstest]
#[case(r#""#, vec![])]
#[case(r#"[features]"#, vec![])]
#[case(r#"[features]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]"#, vec![])]
#[case(r#"[features]
F0 = [
"B/F0",
"A/F0",
]"#, vec!["F0"])]
#[case(r#"[features]
G0 = [
	"B/F0",
	"A/F0",
]"#, vec!["G0"])]
#[case(r#"[features]
G0 = [
	"B/F0",
	"A/F0",
]
F0 = [
	"A/F0",
	"B/F0",
	"C/F0",
]"#, vec!["G0"])]
#[case(r#"[features]
G0 = [
	"B/F0",
	"A/F0",
]
F0 = [
"B/F0",
"A/F0",
]"#, vec!["F0", "G0"])]
fn check_sorted_all_works(#[case] input: &str, #[case] expect: Vec<&str>) {
	let fixer = AutoFixer::from_raw(input).unwrap();
	assert_eq!(fixer.check_sorted_all_features(), expect);
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
	assert!(fixer.check_sorted_all_features().is_empty(), "Features should be sorted");
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
fn canon_some_features_with_modes_works(
	#[case] modes: Vec<(&str, Vec<Mode>)>,
	#[case] input: &str,
	#[case] modify: Option<&str>,
) {
	let mut fixer = AutoFixer::from_raw(input).unwrap();
	let modes = modes.into_iter().map(|(f, m)| (f.into(), m)).collect::<Map<_, _>>();
	fixer.canonicalize_features(&modes, 0).unwrap();
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
