// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use assert_cmd::{assert::OutputAssertExt, Command};
use std::collections::HashMap;

use zepter::mock::*;

#[test]
fn all() {
	let filter = std::env::var("UI_FILTER").unwrap_or_else(|_| "**/*.yaml".into());
	let regex = format!("tests/{}", filter);
	// Loop through all files in tests/ recursively
	let files = glob::glob(&regex).unwrap();
	let overwrite = std::env::var("OVERWRITE").is_ok();
	let keep_going = std::env::var("KEEP_GOING").is_ok();
	let (mut failed, mut good) = (0, 0);

	// Update each time you add a test.
	for file in files.filter_map(Result::ok) {
		let mut config = CaseFile::from_file(&file);
		let (workspace, _ctx) = config.init().unwrap();
		let mut overwrites = HashMap::new();
		let mut diff_overwrites = HashMap::new();
		let m = config.cases().len();

		for (i, case) in config.cases().iter().enumerate() {
			colour::white!("Testing {} {}/{} .. ", file.display(), i + 1, m);
			git_reset(workspace.as_path()).unwrap();
			let mut cmd = Command::cargo_bin("zepter").unwrap();
			for arg in case.cmd.split_whitespace() {
				cmd.arg(arg);
			}
			cmd.args(["--manifest-path", workspace.as_path().to_str().unwrap()]);
			if i > 0 {
				cmd.arg("--offline");
			}

			// remove empty trailing and suffix lines
			let res = cmd.output().unwrap();
			if let Some(code) = case.code {
				res.clone().assert().code(code);
			} else {
				res.clone().assert().success();
			}

			match res.stdout == case.stdout.as_bytes() {
				true => {
					colour::green!("cout:OK");
					colour::white!(" ");
					good += 1;
				},
				false if !overwrite => {
					colour::red!("cout:FAILED");
					if !keep_going {
						pretty_assertions::assert_eq!(
							&String::from_utf8_lossy(&res.stdout),
							&normalize(&case.stdout)
						);
					}
				},
				false => {
					colour::yellow!("cout:OVERWRITE");
					colour::white!(" ");
					overwrites.insert(i, String::from_utf8_lossy(&res.stdout).to_string());
					failed += 1;
				},
			}

			let got = git_diff(workspace.as_path()).unwrap();
			if got != case.diff {
				if std::env::var("OVERWRITE").is_ok() {
					diff_overwrites.insert(i, got);
					colour::yellow_ln!("diff:OVERWRITE");
					colour::white!("");
				} else {
					colour::red_ln!("diff:FAILED");
					colour::white!("");
					if !keep_going {
						pretty_assertions::assert_eq!(got, case.diff);
					}
				}
			} else {
				colour::green_ln!("diff:OK");
				colour::white!("");
			}
			git_reset(workspace.as_path()).unwrap();
		}

		//if std::env::var("PERSIST").is_ok() {
		//	let path = workspace.persist();
		//	println!("Persisted to {:?}", path);
		//}

		if std::env::var("OVERWRITE").is_ok() {
			if overwrites.is_empty() && diff_overwrites.is_empty() {
				continue
			}

			for (i, stdout) in overwrites.into_iter() {
				config.case_mut(i).stdout = stdout;
			}
			for (i, diff) in diff_overwrites.into_iter() {
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
	colour::white!("");
}
