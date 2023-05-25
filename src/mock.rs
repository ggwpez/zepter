// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Helpers for writing tests.

#![cfg(feature = "testing")]

use std::process::Command;
use std::path::Path;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Case {
	pub cmd: String,
	pub stdout: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<i32>,
	pub diff: Option<String>,
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
	Ok(())
}
