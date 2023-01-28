mod dag;

use cargo_metadata::{CargoOpt, *};
use clap::Parser;
use dag::Dag;
use env_logger::Env;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Command {
	#[clap(subcommand)]
	subcommand: SubCommand,

	#[clap(long)]
	verbose: bool,
}

#[derive(Debug, clap::Subcommand)]
enum SubCommand {
	Trace(TraceCmd),
}

#[derive(Debug, Parser)]
pub struct TraceCmd {
	#[arg(long, default_value = "Cargo.toml")]
	pub manifest_path: PathBuf,

	#[clap(long, action)] // TODO make false default
	workspace: bool,

	#[clap(long, short, index(1))]
	from: String,

	#[clap(long, short, index(2))]
	to: String,
}

fn main() {
	env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
	let cmd = Command::parse();

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
		let meta = self.meta_of(&self.manifest_path, CargoOpt::AllFeatures);
		let mut dag = Dag::default();
		for p in meta.packages {
			for dep in p.dependencies {
				if dep.kind == DependencyKind::Normal {
					dag.add_edge(p.name.clone(), dep.name);
				}
			}
		}
		if !dag.contains(&self.from) {
			println!("{} is not a in the workspace", self.from);
			return
		}
		if !dag.contains(&self.to) {
			println!("{} is not a dependency of the workspace", self.to);
			return
		}

		let forward = dag.clone().dag_of(&self.from);
		let depends = forward.into_transitive_hull_in(&dag);

		match depends.connected(&self.from, &self.to) {
			true => log::info!("Calculating shortest path from '{}' to '{}'", self.from, self.to),
			false => {
				panic!("{} does not depend on {}", self.from, self.to);
			},
		}

		let paths = dag.all_paths(&self.from, &self.to);
		let shortest = paths.iter().min_by_key(|p| p.len()).unwrap();
		log::info!("The shortest out of {} paths:", paths.len());
		println!("{} -> {}", self.from, shortest.join(" -> "));
	}

	fn meta_of(&self, manifest_path: &PathBuf, features: CargoOpt) -> Metadata {
		let mut cmd = cargo_metadata::MetadataCommand::new();
		cmd.manifest_path(manifest_path);
		cmd.features(features);
		cmd.no_deps();
		if !self.workspace {
			cmd.no_deps();
		}
		cmd.exec()
			.unwrap_or_else(|_| panic!("Failed to read manifest {:?}", manifest_path))
	}
}
