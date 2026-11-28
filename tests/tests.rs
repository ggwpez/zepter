// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use assert_cmd::{assert::OutputAssertExt, Command};
use std::{path::Path, process::Output};

use zepter::mock::*;

#[test]
#[ignore]
fn integration() {
	// Set backtrace since it appears in the UI test output
	std::env::set_var("RUST_BACKTRACE", "0");

	let filter = std::env::var("UI_FILTER").unwrap_or_else(|_| "**/*.yaml".into());
	let regex = format!("tests/{filter}");
	// Loop through all files in tests/ recursively
	let files = glob::glob(&regex).unwrap();
	let mut run = FixtureRun {
		overwrite: std::env::var("OVERWRITE").is_ok(),
		keep_going: std::env::var("KEEP_GOING").is_ok(),
		good: 0,
		updated: 0,
	};

	if run.overwrite {
		colour::white_ln!("Running tests in OVERWRITE mode\n");
	}

	for file in files.filter_map(Result::ok).filter(|f| f.is_file()) {
		run.check_file(&file);
	}

	if run.updated > 0 {
		println!("Updated {} test(s)", run.updated);
	}
	if run.updated == 0 && run.good == 0 {
		panic!("No tests found. Try something like '**/never-enables/diamond.yaml'");
	}
	println!();
}

struct FixtureRun {
	overwrite: bool,
	keep_going: bool,
	good: usize,
	updated: usize,
}

impl FixtureRun {
	fn check_file(&mut self, file: &Path) {
		let mut config = CaseFile::from_file(file);
		let (workspace, ctx) = config.init().unwrap();
		let default_args = config.default_args();
		let count = config.cases().len();
		let mut changed = false;

		for i in 0..count {
			let case = config.case_mut(i);
			let _init = case.init(&workspace).unwrap();
			colour::white!("{} {}/{} ", file.display(), i + 1, count);
			git_reset(&workspace).unwrap();
			let output = run_case_command(case, &workspace, default_args, i);
			changed |= self.check_output(case, output);
			changed |= self.check_diff(case, &workspace);
			git_reset(&workspace).unwrap();
		}

		if std::env::var("PERSIST").is_ok() {
			if let Some(ctx) = ctx {
				colour::white_ln!("Persisted to {:?}", ctx.persist());
			} else {
				colour::red_ln!("Cannot persist test");
			}
		}
		if changed {
			config.to_file(file).unwrap();
			println!("Updated {}", file.display());
		}
	}

	fn check_output(&mut self, case: &mut Case, output: Output) -> bool {
		// Keep stdout first: overwrite mode updates only the first mismatched stream per run.
		let (label, actual, expected) = if output.stdout != case.stdout.as_bytes() {
			("cout", output.stdout, &mut case.stdout)
		} else if output.stderr != case.stderr.as_bytes() {
			("cerr", output.stderr, &mut case.stderr)
		} else {
			colour::white!("cout:");
			colour::green!("OK");
			colour::white!(" ");
			self.good += 1;
			return false
		};

		if self.overwrite {
			colour::white!("{label}:");
			colour::yellow!("OVERWRITE");
			colour::white!(" ");
			*expected = String::from_utf8_lossy(&actual).into_owned();
			self.updated += 1;
			return true
		}

		colour::white!("cerr:");
		colour::red!("FAIL");
		colour::white!(" ");
		if !self.keep_going {
			pretty_assertions::assert_eq!(&String::from_utf8_lossy(&actual), &normalize(expected));
			unreachable!()
		}
		false
	}

	fn check_diff(&self, case: &mut Case, workspace: &Path) -> bool {
		let got = git_diff(workspace).unwrap();
		colour::white!("diff:");
		if got == case.diff {
			colour::green_ln!("OK");
		} else if self.overwrite {
			colour::yellow_ln!("OVERWRITE");
			case.diff = got;
			return true
		} else {
			colour::red_ln!("FAILED");
			if !self.keep_going {
				assert_diff_matches(&got, &case.diff);
			}
		}
		false
	}
}

fn run_case_command(case: &Case, workspace: &Path, default_args: bool, index: usize) -> Output {
	let mut cmd = Command::new(assert_cmd::cargo_bin!("zepter"));
	for arg in case.cmd.split_whitespace() {
		cmd.arg(arg);
	}

	if default_args {
		let toml_path = workspace.join("Cargo.toml");
		cmd.args(["--manifest-path", toml_path.as_path().to_str().unwrap(), "--log", "warn"]);
		if index > 0 {
			cmd.arg("--offline");
		}
	} else {
		cmd.current_dir(workspace);
	}

	// remove empty trailing and suffix lines
	let res = cmd.output().unwrap();
	if let Some(code) = case.code {
		res.clone().assert().code(code);
	} else {
		res.clone().assert().success();
	}
	res
}

fn assert_diff_matches(actual: &str, expected: &str) {
	let actual: Vec<_> = actual.lines().collect();
	let expected: Vec<_> = expected.lines().collect();
	for (got, want) in actual.chunks(10).zip(expected.chunks(10)) {
		pretty_assertions::assert_eq!(got, want);
	}
	unreachable!()
}

#[test]
fn cargo_metadata_error_is_formatted_without_panic() {
	let dir = std::env::temp_dir().join(format!("zepter-metadata-err-{}", std::process::id()));
	let _ = std::fs::remove_dir_all(&dir);
	std::fs::create_dir_all(&dir).unwrap();
	let manifest = dir.join("Cargo.toml");
	std::fs::write(&manifest, "[package]\nname = \"broken\"\nversion = \"not-a-semver\"\n")
		.unwrap();

	let output = Command::new(assert_cmd::cargo_bin!("zepter"))
		.args(["lint", "why-enabled", "--package", "foo", "--feature", "bar", "--manifest-path"])
		.arg(&manifest)
		.output()
		.unwrap();

	let _ = std::fs::remove_dir_all(&dir);

	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("Failed to load metadata (`cargo metadata` failed):\n"),
		"unexpected stderr:\n{stderr}"
	);
	assert!(!stderr.contains("panicked at"), "should not panic:\n{stderr}");
	assert!(!stderr.contains("\\n"), "should not escape newlines:\n{stderr}");
}

#[test]
fn workflow_lookup_failures_exit_with_config_error() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("zepter.yaml");
	let run = || {
		let output = Command::new(assert_cmd::cargo_bin!("zepter"))
			.args(["run", "missing", "--config"])
			.arg(&path)
			.output()
			.unwrap();
		let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
		assert!(!stderr.contains("panicked at"), "should not panic:\n{stderr}");
		(output.status.code(), stderr)
	};

	let (code, stderr) = run();
	assert_eq!(code, Some(101), "stderr:\n{stderr}");
	assert!(stderr.contains("Provided config path does not exist"), "stderr:\n{stderr}");

	std::fs::write(&path, "version: {format: 1, binary: 0.1.0}\nworkflows: {}\n").unwrap();
	let (code, stderr) = run();
	assert_eq!(code, Some(101), "stderr:\n{stderr}");
	assert!(stderr.contains("Workflow 'missing' not found"), "stderr:\n{stderr}");
}

#[test]
fn lift_preserves_both_edits_to_root_package_manifest() {
	let dir = tempfile::tempdir().unwrap();
	let root = dir.path();
	let manifest = root.join("Cargo.toml");
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(root.join("src/lib.rs"), "").unwrap();
	std::fs::write(
		&manifest,
		r#"[package]
name = "zapp"
version = "0.1.0"
edition = "2021"

[workspace]
members = ["helper"]

[dependencies]
helper = { path = "helper" }
"#,
	)
	.unwrap();
	std::fs::create_dir_all(root.join("helper/src")).unwrap();
	std::fs::write(root.join("helper/src/lib.rs"), "").unwrap();
	std::fs::write(
		root.join("helper/Cargo.toml"),
		"[package]\nname = \"helper\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
	)
	.unwrap();

	Command::new(assert_cmd::cargo_bin!("zepter"))
		.current_dir(root)
		.args(["transpose", "dependency", "lift-to-workspace", "helper", "--offline", "--fix"])
		.assert()
		.success();

	let doc: toml_edit::DocumentMut = std::fs::read_to_string(&manifest).unwrap().parse().unwrap();
	assert_eq!(doc["workspace"]["dependencies"]["helper"]["path"].as_str(), Some("helper"));
	assert_eq!(doc["dependencies"]["helper"]["workspace"].as_bool(), Some(true));

	let meta = cargo_metadata::MetadataCommand::new()
		.manifest_path(&manifest)
		.other_options(vec!["--offline".into()])
		.exec()
		.unwrap();
	let root_id = &meta.root_package().unwrap().id;
	let helper_id = &meta.packages.iter().find(|p| p.name == "helper").unwrap().id;
	let root_node = meta.resolve.as_ref().unwrap().nodes.iter().find(|n| &n.id == root_id).unwrap();
	assert!(root_node.dependencies.contains(helper_id));
}

#[test]
fn propagation_preserves_namespaced_optional_dependency_activation() {
	let dir = tempfile::tempdir().unwrap();
	let root = dir.path();
	for src in ["src", "helper/src"] {
		std::fs::create_dir_all(root.join(src)).unwrap();
		std::fs::write(root.join(src).join("lib.rs"), "").unwrap();
	}
	std::fs::write(
		root.join("helper/Cargo.toml"),
		"[package]\nname = 'helper'\nversion = '0.1.0'\nedition = '2021'\n[features]\nextra = []\n",
	)
	.unwrap();
	let manifest = root.join("Cargo.toml");
	let original = "[package]
name = 'app'
version = '0.1.0'
edition = '2021'
[dependencies]
alias = { package = 'helper', path = 'helper', optional = true }
[features]
enabled = ['dep:alias']
missing = ['alias?/extra']
";
	std::fs::write(&manifest, original).unwrap();
	let fix = |feature: &str| {
		Command::new(assert_cmd::cargo_bin!("zepter"))
			.current_dir(root)
			.args(["lint", "propagate-feature", "--offline", "--fix", "-p", "app", "--feature"])
			.arg(feature)
			.args(["--feature-enables-dep", &format!("{feature}:alias")])
			.assert()
			.success();
	};

	fix("enabled");
	assert_eq!(std::fs::read_to_string(&manifest).unwrap(), original);
	fix("missing");
	let fixed = std::fs::read_to_string(&manifest).unwrap();
	let meta = cargo_metadata::MetadataCommand::new()
		.manifest_path(&manifest)
		.other_options(vec!["--offline".into(), "--features=missing".into()])
		.exec()
		.unwrap();
	let app = meta.root_package().unwrap();
	assert_eq!(app.features["missing"], ["alias?/extra", "dep:alias"]);
	assert!(!app.features.contains_key("alias"));
	let app_node = meta.resolve.as_ref().unwrap().nodes.iter().find(|n| n.id == app.id).unwrap();
	assert!(app_node.deps.iter().any(|dep| dep.name == "alias"));
	fix("missing");
	assert_eq!(std::fs::read_to_string(manifest).unwrap(), fixed);
}
