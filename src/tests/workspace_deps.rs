// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use crate::{autofix::AutoFixer, kind_to_str};
use cargo_metadata::DependencyKind::*;
use rstest::rstest;

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
	Err("Cannot lift git dependencies")
)]
#[case(
	r#"[dependencies]
log = { random = "321", default-features = true, hey = true, path = "123" }
"#,
	Some(true),
	Err("Lifting dependency would change it from a path dependency to a crates-io dependency")
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
		let res = fixer.lift_dependency(
			"log",
			&kind,
			default,
			&crate::cmd::transpose::SourceLocationSelector::Remote,
		);

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
