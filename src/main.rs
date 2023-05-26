// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Entry point of the program.

use clap::Parser;
use zepter::cmd::Command;

fn main() {
	let cmd = Command::parse();
	env_logger::init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "debug"),
	);
	cmd.run();
}
