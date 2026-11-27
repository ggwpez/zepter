// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use assert_cmd::{assert::OutputAssertExt, Command};
use std::{collections::HashMap, path::Path, process::Output};

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
	let overwrite = std::env::var("OVERWRITE").is_ok();
	let keep_going = std::env::var("KEEP_GOING").is_ok();
	let (mut failed, mut good) = (0, 0);

	if overwrite {
		colour::white_ln!("Running tests in OVERWRITE mode\n");
	}

	// Update each time you add a test.
	for file in files.filter_map(Result::ok).filter(|f| f.is_file()) {
		let mut config = CaseFile::from_file(&file);
		let (workspace, ctx) = config.init().unwrap();
		let mut cout_overwrites = HashMap::new();
		let mut cerr_overwrites = HashMap::new();
		let mut diff_overwrites = HashMap::new();
		let m = config.cases().len();

		for (i, case) in config.cases().iter().enumerate() {
			let _init = case.init(workspace.as_path()).unwrap();
			colour::white!("{} {}/{} ", file.display(), i + 1, m);
			git_reset(workspace.as_path()).unwrap();
			let res = run_case_command(case, &workspace, config.default_args(), i);

			match (res.stdout == case.stdout.as_bytes(), res.stderr == case.stderr.as_bytes()) {
				(true, true) => {
					colour::white!("cout:");
					colour::green!("OK");
					colour::white!(" ");
					good += 1;
				},
				(false, _) if !overwrite => {
					colour::white!("cerr:");
					colour::red!("FAIL");
					colour::white!(" ");
					if !keep_going {
						pretty_assertions::assert_eq!(
							&String::from_utf8_lossy(&res.stdout),
							&normalize(&case.stdout),
						);
						unreachable!()
					}
				},
				(true, false) if !overwrite => {
					colour::white!("cerr:");
					colour::red!("FAIL");
					colour::white!(" ");
					if !keep_going {
						pretty_assertions::assert_eq!(
							&String::from_utf8_lossy(&res.stderr),
							&normalize(&case.stderr),
						);
						unreachable!()
					}
				},
				(true, false) => {
					colour::white!("cerr:");
					colour::yellow!("OVERWRITE");
					colour::white!(" ");
					cerr_overwrites.insert(i, String::from_utf8_lossy(&res.stderr).to_string());

					failed += 1;
				},
				(false, _) => {
					colour::white!("cout:");
					colour::yellow!("OVERWRITE");
					colour::white!(" ");
					cout_overwrites.insert(i, String::from_utf8_lossy(&res.stdout).to_string());

					failed += 1;
				},
			}

			check_diff(case, &workspace, keep_going, &mut diff_overwrites, i);
			git_reset(workspace.as_path()).unwrap();
		}

		if std::env::var("PERSIST").is_ok() {
			if let Some(ctx) = ctx {
				let path = ctx.persist();
				colour::white_ln!("Persisted to {:?}", path);
			} else {
				colour::red_ln!("Cannot persist test");
			}
		}

		if std::env::var("OVERWRITE").is_ok() {
			if cout_overwrites.is_empty() &&
				cerr_overwrites.is_empty() &&
				diff_overwrites.is_empty()
			{
				continue
			}

			for (i, stdout) in cout_overwrites {
				config.case_mut(i).stdout = stdout;
			}
			for (i, stderr) in cerr_overwrites {
				config.case_mut(i).stderr = stderr;
			}
			for (i, diff) in diff_overwrites {
				config.case_mut(i).diff = diff;
			}

			config.to_file(&file).unwrap();
			println!("Updated {}", file.display());
		}
	}

	if failed > 0 {
		if std::env::var("OVERWRITE").is_ok() {
			println!("Updated {failed} test(s)");
		} else {
			panic!("{failed} test(s) failed");
		}
	}
	if failed == 0 && good == 0 {
		panic!("No tests found. Try something like '**/never-enables/diamond.yaml'");
	}
	println!();
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

fn check_diff(
	case: &Case,
	workspace: &Path,
	keep_going: bool,
	diff_overwrites: &mut HashMap<usize, String>,
	index: usize,
) {
	let got = git_diff(workspace).unwrap();
	if got != case.diff {
		if std::env::var("OVERWRITE").is_ok() {
			diff_overwrites.insert(index, got);
			colour::white!("diff:");
			colour::yellow_ln!("OVERWRITE");
			colour::white!("");
		} else {
			colour::white!("diff:");
			colour::red_ln!("FAILED");
			colour::white!("");
			if !keep_going {
				// assert in chunks of 10 lines
				let got_lines = got.lines().collect::<Vec<_>>();
				let got_chunks = got_lines.chunks(10).collect::<Vec<_>>();
				let case_lines = case.diff.lines().collect::<Vec<_>>();
				let case_chunks = case_lines.chunks(10).collect::<Vec<_>>();

				for (got_chunk, case_chunk) in got_chunks.iter().zip(case_chunks.iter()) {
					pretty_assertions::assert_eq!(got_chunk, case_chunk);
				}
				unreachable!()
			}
		}
	} else {
		colour::white!("diff:");
		colour::green_ln!("OK");
		colour::white!("");
	}
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
