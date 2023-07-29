// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Helpers for cloning and checking out git repositories.

use std::{path::Path, path::PathBuf, process::Command};

/// Create a mocked git repository.
pub fn git_init(dir: &Path) -> Result<(), anyhow::Error> {
	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("init");
	cmd.arg("--quiet");
	cmd.status()?;

	// Do an init commit
	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("add");
	cmd.arg("--all");
	cmd.status()?;

	// git config user.email "you@example.com"
	// git config user.name "Your Name"
	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("config");
	cmd.arg("user.email");
	cmd.arg("you@example.com");
	cmd.status()?;

	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("config");
	cmd.arg("user.name");
	cmd.arg("Your Name");
	cmd.status()?;

	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("commit");
	cmd.arg("--message");
	cmd.arg("init");
	cmd.arg("--author");
	cmd.arg("test <t@t.com>");
	cmd.arg("--no-gpg-sign");
	cmd.arg("--quiet");
	cmd.status()?;

	Ok(())
}

pub fn git_diff(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("diff");
	cmd.arg("--abbrev=6"); // Pick a deterministic commit hash len.
	cmd.arg("--patch");
	cmd.arg("--no-color");
	cmd.arg("--minimal");
	cmd.arg("--no-prefix");
	let output = cmd.output()?;
	Ok(String::from_utf8_lossy(&output.stdout).into())
}

pub fn git_reset(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("checkout");
	cmd.arg("--");
	cmd.arg(".");
	cmd.status()?;

	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("reset");
	cmd.arg("--hard");
	cmd.arg("--quiet");
	cmd.status()?;
	Ok(())
}

pub fn clone_repo(repo: &str, rev: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
	let dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
	let repos_dir = std::path::Path::new(&dir).join("test-repos");
	let dir = repos_dir.join(repo);

	// Check if the repo is already cloned
	if Path::new(&dir).exists() {
	} else {
		std::fs::create_dir_all(&dir)?;

		let mut cmd = Command::new("git");
		cmd.current_dir(&dir);
		cmd.arg("init");
		cmd.arg("--quiet");
		cmd.status()?;

		// add remote
		let mut cmd = Command::new("git");
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

pub fn fetch(dir: &PathBuf, rev: &str) -> Result<(), Box<dyn std::error::Error>> {
	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("fetch");
	cmd.arg("--depth");
	cmd.arg("1");
	cmd.arg("origin");
	cmd.arg(rev);
	cmd.status()?;
	Ok(())
}

pub fn checkout(dir: &PathBuf, rev: &str) -> Result<(), Box<dyn std::error::Error>> {
	let mut cmd = Command::new("git");
	cmd.current_dir(dir);
	cmd.arg("checkout");
	cmd.arg(rev);
	cmd.status()?;
	Ok(())
}
