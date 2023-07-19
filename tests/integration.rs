// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use assert_cmd::{assert::OutputAssertExt, Command};
use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
};
use zepter::mock::*;

pub type ModuleName = String;
pub type FeatureName = String;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Repo {
	pub name: String,
	#[serde(rename = "ref")]
	pub ref_spec: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CaseFile {
	pub repo: Repo,
	pub cases: Vec<Case>,
}

impl CaseFile {
	pub fn from_file(path: &Path) -> Self {
		let content = fs::read_to_string(path).unwrap();
		let content = content.replace('\t', "  ");
		serde_yaml::from_str(&content)
			.unwrap_or_else(|_| panic!("Failed to parse: {}", &path.display()))
	}

	pub fn init(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
		clone_repo(&self.repo.name, &self.repo.ref_spec)
	}
}

#[test]
fn integration() {
	let filter = std::env::var("UI_FILTER").unwrap_or_else(|_| "**/*.yaml".into());
	let regex = format!("tests/integration/{}", filter);
	// Loop through all files in tests/ recursively
	let files = glob::glob(&regex).unwrap();
	let overwrite = std::env::var("OVERWRITE").is_ok();
	let (mut failed, mut good) = (0, 0);

	// Update each time you add a test.
	for file in files.filter_map(Result::ok) {
		let mut config = CaseFile::from_file(&file);
		let workspace = config.init().unwrap();
		let mut overwrites = HashMap::new();
		let m = config.cases.len();

		for (i, case) in config.cases.iter().enumerate() {
			colour::white!("Testing {} {}/{} .. ", file.display(), i + 1, m);
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
					colour::green_ln!("OK");
					colour::white!("");
					good += 1;
				},
				false if !overwrite => {
					colour::red_ln!("FAILED");
					pretty_assertions::assert_eq!(
						&String::from_utf8_lossy(&res.stdout),
						&normalize(&case.stdout)
					);
				},
				false => {
					colour::yellow_ln!("OVERWRITE");
					colour::white!("");
					overwrites.insert(i, String::from_utf8_lossy(&res.stdout).to_string());
					failed += 1;
				},
			}
		}

		if std::env::var("OVERWRITE").is_ok() {
			if overwrites.is_empty() {
				continue
			}

			for (i, stdout) in overwrites.into_iter() {
				config.cases[i].stdout = stdout;
			}

			let mut fd = fs::File::create(&file).unwrap();
			serde_yaml::to_writer(&mut fd, &config).unwrap();
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

pub(crate) fn clone_repo(repo: &str, rev: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
	let dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
	let repos_dir = std::path::Path::new(&dir).join("test-repos");
	let dir = repos_dir.join(repo);

	// Check if the repo is already cloned
	if std::path::Path::new(&dir).exists() {
	} else {
		std::fs::create_dir_all(&dir)?;

		let mut cmd = std::process::Command::new("git");
		cmd.current_dir(&dir);
		cmd.arg("init");
		cmd.arg("--quiet");
		cmd.status()?;

		// add remote
		let mut cmd = std::process::Command::new("git");
		cmd.current_dir(&dir);
		cmd.arg("remote");
		cmd.arg("add");
		cmd.arg("origin");
		cmd.arg(&format!("https://github.com/paritytech/{}", repo));
		cmd.status()?;

		fetch(&dir, rev)?;
	}

	if checkout(&dir, rev).is_err() {
		fetch(&dir, rev)?;
		checkout(&dir, rev)?;
	}
	Ok(dir)
}

fn fetch(dir: &PathBuf, rev: &str) -> Result<(), Box<dyn std::error::Error>> {
	let mut cmd = std::process::Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("fetch");
	cmd.arg("--depth");
	cmd.arg("1");
	cmd.arg("origin");
	cmd.arg(rev);
	cmd.assert().try_code(0)?;
	Ok(())
}

fn checkout(dir: &PathBuf, rev: &str) -> Result<(), Box<dyn std::error::Error>> {
	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("checkout");
	cmd.arg(rev);
	cmd.assert().try_code(0)?;
	Ok(())
}
