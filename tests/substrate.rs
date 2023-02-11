// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Integration tests using the Substrate repo.

#![cfg(test)]

use assert_cmd::cargo::cargo_bin;
use std::path::PathBuf;

// mutex for repo init by default
lazy_static::lazy_static! {
	static ref SUBSTRATE: std::sync::Mutex<PathBuf> = std::sync::Mutex::new(clone_repo("substrate", "master").unwrap());
}

#[test]
fn substrate_trace_works() {
	let substrate = SUBSTRATE.lock().unwrap();

	let mut cmd = std::process::Command::new(cargo_bin("feature"));
	cmd.arg("trace");
	cmd.arg("--manifest-path");
	cmd.arg(substrate.join("Cargo.toml"));
	cmd.arg("node-cli");
	cmd.arg("snow");

	let output = cmd.output().unwrap();
	if !output.status.success() {
		panic!(
			"Command failed with status {:?}: {}",
			output.status,
			String::from_utf8_lossy(&output.stderr)
		);
	}
	let stdout = String::from_utf8_lossy(&output.stdout);
	let want = "node-cli -> try-runtime-cli -> substrate-rpc-client -> sp-runtime -> substrate-test-runtime-client -> substrate-test-runtime -> sc-service -> sc-telemetry -> libp2p -> libp2p-webrtc -> libp2p-noise -> snow";
	if !stdout.contains(want) {
		panic!("Unexpected output: {stdout}");
	}
}

#[test]
fn substrate_lint_works() {
	let substrate = SUBSTRATE.lock().unwrap();

	let mut cmd = std::process::Command::new(cargo_bin("feature"));
	cmd.args(["lint", "propagate-feature"]);
	cmd.arg("--manifest-path");
	cmd.arg(substrate.join("Cargo.toml"));
	cmd.arg("--workspace");
	cmd.args(["--feature", "runtime-benchmarks"]);

	let output = cmd.output().unwrap();
	if !output.status.success() {
		panic!(
			"Command failed with status {:?}: {}",
			output.status,
			String::from_utf8_lossy(&output.stderr)
		);
	}
	let stdout = String::from_utf8_lossy(&output.stdout);
	let want = "Generated 185 errors and 0 warnings and fixed 0 issues.";
	if !stdout.contains(want) {
		panic!("Unexpected output: {stdout}");
	}
}

fn clone_repo(repo: &str, rev: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
	let dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
	let dir = std::path::Path::new(&dir).join("test-repos").join(repo);

	// Check if the repo is already cloned
	if std::path::Path::new(&dir).exists() {
		println!("Using existing repo at {dir:?}");
		return Ok(dir)
	}

	println!("Cloning {repo} into {dir:?}");
	std::fs::create_dir_all(&dir)?;

	let mut cmd = std::process::Command::new("git");
	cmd.current_dir(&dir);
	cmd.arg("clone");
	cmd.arg(format!("https://github.com/paritytech/{repo}"));
	cmd.arg(".");
	cmd.arg("--depth");
	cmd.arg("1");
	cmd.arg("--branch");
	cmd.arg(rev);
	cmd.output()?;
	Ok(dir.clone())
}
