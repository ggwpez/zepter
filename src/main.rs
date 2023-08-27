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
		use env_logger::fmt::Color;
		use std::io::Write;

		env_logger::builder()
			.parse_env(
				env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "debug"),
			)
			.format_timestamp(None)
			.format(|buf, record| {
				let mut level_style = buf.style();
				level_style.set_bold(true);

				match record.level() {
					log::Level::Error => {
						level_style.set_color(Color::Red);
					},
					log::Level::Warn => {
						level_style.set_color(Color::Yellow);
					},
					log::Level::Info => {
						level_style.set_color(Color::White);
					},
					log::Level::Debug => {
						level_style.set_color(Color::Blue);
					},
					log::Level::Trace => {
						level_style.set_color(Color::Magenta);
					},
				};

				writeln!(buf, "[{}] {}", level_style.value(record.level()), record.args())
			})
			.init();
	}
}
