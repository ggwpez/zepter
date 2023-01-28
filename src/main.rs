use cargo_metadata::{CargoOpt, *};
use clap::Parser;
use env_logger::Env;
use feature::DAG;
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
	Show(ShowCmd),
}

#[derive(Debug, Parser)]
pub struct ShowCmd {
	#[arg(long, default_value = "Cargo.toml")]
	pub manifest_path: PathBuf,

	#[clap(long, action)] // TODO make false default
	workspace: bool,

	#[clap(long, short)]
	root: String,

	#[clap(long, short)]
	package: String,
}

fn main() {
	env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
	let cmd = Command::parse();

	match cmd {
		Command { subcommand: SubCommand::Show(cmd), .. } => {
			cmd.run();
		},
	}
}

impl ShowCmd {
	fn run(&self) {
		use cargo_metadata::*;

		log::info!("Using manifest {:?}", self.manifest_path);
		let meta = self.meta_of(&self.manifest_path, CargoOpt::AllFeatures);
		let mut dag = DAG::default();
		for p in meta.packages {
			for dep in p.dependencies {
				if dep.kind == DependencyKind::Normal {
					dag.add_edge(p.name.clone(), dep.name);
				}
			}
		}
		if !dag.contains(&self.root) {
			println!("{} is not a dependency of the workspace", self.root);
			return
		}
		if !dag.contains(&self.package) {
			println!("{} is not a dependency of the workspace", self.package);
			return
		}

		let forward = dag.clone().dag_of(&self.root);
		let depends = forward.into_transitive_hull_in(&dag);

		match depends.connected(&self.root, &self.package) {
			true => println!("Calculating shortest path from {} to {}...", self.root, self.package),
			false => {
				println!("{} does not depend on {}", self.root, self.package);
				return
			},
		}

		let paths = dag.all_paths(&self.root, &self.package);
		let shortest = paths.iter().min_by_key(|p| p.len()).unwrap();
		println!("Found {} paths in total", paths.len());
		println!("{} -> {}", self.root, shortest.join(" -> "));
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
