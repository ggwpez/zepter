// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{
	autofix::AutoFixer,
	cmd::fmt::Mode::{self, Canonicalize, Dedub, Sort},
};
use rstest::rstest;
use std::{collections::BTreeMap as Map, vec};

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
