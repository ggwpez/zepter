// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::autofix::AutoFixer;
use rstest::rstest;

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
