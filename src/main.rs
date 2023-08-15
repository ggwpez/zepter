// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Entry point of the program.

use clap::Parser;
use zepter::cmd::Command;

fn main() {
	setup_logging();

	let cmd = Command::parse();
	cmd.run();
}

fn setup_logging() {
	#[cfg(feature = "logging")]
	{
		use std::io::Write;
		env_logger::builder()
			.parse_env(
				env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "debug"),
			)
			.format_timestamp(None)
			.format(|buf, record| {
				let mut level_style = buf.style();
				level_style.set_bold(true);
				writeln!(buf, "[{}] {}", level_style.value(record.level()), record.args())
			})
			.init();
	}
}
