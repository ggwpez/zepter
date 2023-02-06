pub mod lint;
pub mod trace;

use cargo_metadata::{Metadata, MetadataCommand};

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

#[derive(Debug, clap::Parser)]
pub struct TreeArgs {
	/// Cargo manifest path.
	#[arg(long, global = true, default_value = "Cargo.toml")]
	pub manifest_path: std::path::PathBuf,

	/// Whether to only consider workspace crates.
	#[clap(long, global = true, default_value = "false")]
	pub workspace: bool,
}

impl TreeArgs {
	pub fn load_metadata(&self) -> Result<Metadata, String> {
		let mut cmd = MetadataCommand::new();
		cmd.manifest_path(&self.manifest_path);
		cmd.features(cargo_metadata::CargoOpt::AllFeatures);

		if self.workspace {
			cmd.no_deps();
		}
		cmd.exec().map_err(|e| format!("Failed to load metadata: {}", e))
	}
}
