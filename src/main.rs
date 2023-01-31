mod dag;

use cargo_metadata::{CargoOpt, *};
use clap::Parser;
use dag::Dag;
use env_logger::Env;
use std::path::PathBuf;

/// See out how Rust dependencies and features are enabled.
#[derive(Debug, Parser)]
struct Command {
	#[clap(subcommand)]
	subcommand: SubCommand,

	#[clap(long, global = true)]
	quiet: bool,
}

#[derive(Debug, clap::Subcommand)]
enum SubCommand {
	Trace(TraceCmd),
}

/// Trace the dependency path one crate to another.
#[derive(Debug, Parser)]
pub struct TraceCmd {
	/// Cargo manifest path.
	#[arg(long, default_value = "Cargo.toml")]
	manifest_path: PathBuf,

	/// Whether to only consider workspace crates.
	#[clap(long, default_value = "false")]
	workspace: bool,

	/// The root crate to start from.
	#[clap(long, short, index(1))]
	from: String,

	/// The dependency crate to end at.
	#[clap(long, short, index(2))]
	to: String,
}

fn main() {
	let cmd = Command::parse();
	let default_log = if cmd.quiet { "warn" } else { "info" };
	env_logger::Builder::from_env(Env::default().default_filter_or(default_log)).init();

	match cmd {
		Command { subcommand: SubCommand::Trace(cmd), .. } => {
			cmd.run();
		},
	}
}

impl TraceCmd {
	fn run(&self) {
		use cargo_metadata::*;

		log::info!("Using manifest {:?}", self.manifest_path);
		let meta = self.metadata(&self.manifest_path, CargoOpt::AllFeatures);
		let mut dag = Dag::<String>::default();
		for p in meta.packages {
			for dep in p.dependencies {
				if dep.kind == DependencyKind::Normal {
					dag.add_edge(p.name.clone(), dep.name);
				}
			}
		}
		if !dag.lhs_contains(&self.from) {
			println!("{} is not in the workspace", self.from);
			return
		}
		if !dag.rhs_contains(&self.to) {
			println!("{} is not a dependency in the workspace", self.to);
			return
		}

		log::info!("Calculating path from {} to {}", self.from, self.to);
		match dag.any_path(&self.from, &self.to) {
			Some(path) => println!("{path}"),
			None => {
				let forward = dag.node(self.from.clone());
				let depends = forward.into_transitive_hull_in(&dag);

				if depends.connected(&self.from, &self.to) {
					unreachable!(
						"Sanity check failed: {} depends on {} but no path could be found",
						self.from, self.to
					);
				}
			},
		}
	}

	fn metadata(&self, manifest_path: &PathBuf, features: CargoOpt) -> Metadata {
		let mut cmd = cargo_metadata::MetadataCommand::new();
		cmd.manifest_path(manifest_path);
		cmd.features(features);
		if self.workspace {
			cmd.no_deps();
		}
		cmd.exec()
			.unwrap_or_else(|_| panic!("Failed to read manifest {manifest_path:?}"))
	}
}
