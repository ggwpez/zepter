// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

//! Entry point of the program.

use clap::Parser;
use zepter::cmd::Command;

fn main() -> Result<(), ()> {
	setup_logging();

	// Need to remove this in case `cargo-zepter` is used:
	let mut args = std::env::args().collect::<Vec<_>>();
	if args.len() > 1 && args[1] == "zepter" {
		args.remove(1);
	}

	if let Err(err) = Command::parse_from(args).run() {
		eprintln!("{}", err);
		Err(())
	} else {
		Ok(())
	}
}

#[cfg(not(feature = "logging"))]
fn setup_logging() {}

#[cfg(feature = "logging")]
fn setup_logging() {
	use std::io::Write;

	env_logger::builder()
		.parse_env(env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "debug"))
		.format_timestamp(None)
		.format(|buf, record| {
			let level_style = buf.default_level_style(record.level()).bold();
			let begin = level_style.render();
			let reset = level_style.render_reset();

			writeln!(buf, "[{begin}{}{reset}] {}", record.level(), record.args())
		})
		.init();
}
