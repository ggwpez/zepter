pub mod lint;
pub mod trace;
pub mod common;

use regex::*;

/// See out how Rust dependencies and features are enabled.
#[derive(Debug, clap::Parser)]
pub struct Command {
	#[clap(subcommand)]
	subcommand: SubCommand,

	#[clap(long, global = true)]
	quiet: bool,
}

#[derive(Debug, clap::Subcommand)]
enum SubCommand {
	Trace(trace::TraceCmd),
	Lint(lint::LintCmd),
}

impl Command {
	pub fn run(&self) {
		if self.quiet {
			log::set_max_level(log::LevelFilter::Error);
		} else {
			log::set_max_level(log::LevelFilter::Info);
		}
		
		match &self.subcommand {
			SubCommand::Trace(cmd) => cmd.run(),
			SubCommand::Lint(cmd) => cmd.run(),
		}
	}
}
