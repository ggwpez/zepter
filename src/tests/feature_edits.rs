// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::autofix::AutoFixer;
use rstest::rstest;

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
