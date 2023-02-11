// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use clap::Parser;
use feature::cmd::Command;

fn main() {
	let cmd = Command::parse();
	env_logger::Builder::from_env(env_logger::Env::default()).init();
	cmd.run();
}
