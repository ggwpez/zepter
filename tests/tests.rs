// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use assert_cmd::{assert::OutputAssertExt, Command};
use std::collections::HashMap;

use zepter::mock::*;

#[test]
#[ignore]
fn integration() {
	let filter = std::env::var("UI_FILTER").unwrap_or_else(|_| "**/*.yaml".into());
	let regex = format!("tests/{}", filter);
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
			let mut cmd = Command::cargo_bin("zepter").unwrap();
			for arg in case.cmd.split_whitespace() {
				cmd.arg(arg);
			}

			if config.default_args() {
				let toml_path = workspace.as_path().join("Cargo.toml");
				cmd.args([
					"--manifest-path",
					toml_path.as_path().to_str().unwrap(),
					"--log",
					"warn",
				]);
				if i > 0 {
					cmd.arg("--offline");
				}
			} else {
				cmd.current_dir(workspace.as_path());
			}

			dbg!(format!("{:?}", cmd));
			// remove empty trailing and suffix lines
			let res = cmd.output().unwrap();
			if let Some(code) = case.code {
				res.clone().assert().code(code);
			} else {
				res.clone().assert().success();
			}

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

			let got = git_diff(workspace.as_path()).unwrap();
			if got != case.diff {
				if std::env::var("OVERWRITE").is_ok() {
					diff_overwrites.insert(i, got);
					colour::white!("diff:");
					colour::yellow_ln!("OVERWRITE");
					colour::white!("");
				} else {
					colour::white!("diff:");
					colour::red_ln!("FAILED");
					colour::white!("");
					if !keep_going {
						pretty_assertions::assert_eq!(got, case.diff);
					}
				}
			} else {
				colour::white!("diff:");
				colour::green_ln!("OK");
				colour::white!("");
			}
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
			println!("Updated {} test(s)", failed);
		} else {
			panic!("{} test(s) failed", failed);
		}
	}
	if failed == 0 && good == 0 {
		panic!("No tests found");
	}
	colour::prnt!("");
}
