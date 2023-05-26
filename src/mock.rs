// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Helpers for writing tests.

#![cfg(feature = "testing")]

use std::{path::Path, process::Command};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Case {
	pub cmd: String,
	#[serde(skip_serializing_if = "String::is_empty")]
	#[serde(default)]
	pub stdout: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<i32>,
	#[serde(skip_serializing_if = "String::is_empty")]
	#[serde(default)]
	pub diff: String,
}

/// Removes leading and trailing empty lines.
pub fn normalize(s: &str) -> String {
	let mut lines = s.lines().collect::<Vec<_>>();
	while lines.first().map(|l| l.is_empty()).is_some() {
		lines.remove(0);
	}
	while lines.last().map(|l| l.is_empty()).is_some() {
		lines.pop();
	}
	format!("{}\n", lines.join("\n"))
}

pub fn git_init(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
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
	cmd.arg("reset");
	cmd.arg("--hard");
	cmd.arg("--quiet");
	cmd.status()?;
	Ok(())
}
