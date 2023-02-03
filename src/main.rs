mod dag;

use clap::Parser;
use dag::Dag;
use env_logger::Env;
use std::path::PathBuf;
use core::fmt::{Formatter, Display};
use core::str::Lines;

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
	#[clap(index(1))]
	from: String,

	/// The dependency crate to end at.
	#[clap(index(2))]
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

#[derive(Clone)]
pub struct Crate {
	name: String,
	version: String,
	features: Vec<String>,
}

impl PartialOrd for Crate {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.name.cmp(&other.name).then(self.version.cmp(&other.version)))
	}
}

impl Ord for Crate {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.name.cmp(&other.name).then(self.version.cmp(&other.version))
	}
}

// only compare name
impl PartialEq for Crate {
	fn eq(&self, other: &Self) -> bool {
		self.name == other.name && self.version == other.version
	}
}

impl Eq for Crate {}

impl Display for Crate {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{} version={}", self.name, self.version)?;
		if !self.features.is_empty() {
			write!(f, "({})", self.features.join(", "))?;
		}
		Ok(())
	}
}

impl TraceCmd {
	fn run(&self) {

		log::info!("Using manifest {:?}", self.manifest_path);
		let _tree = self.tree(&self.from);
		/*let meta = self.metadata(&self.manifest_path, CargoOpt::SomeFeatures(vec!["runtime-benchmarks".into()]));
		let mut dag = Dag::<Crate>::default();
		for p in meta.packages {
			for dep in p.dependencies {
				if dep.kind == DependencyKind::Normal {
					if dep.name == "pallet-xcm" {
						println!("{:?}", &dep.features);
					}
					dag.add_edge(Crate { name: p.name.clone(), features: vec![]},
					Crate {name: dep.name, features: dep.features});
				}
			}
		}
		let from = Crate { name: self.from.clone(), features: vec![] };
		if !dag.lhs_contains(&from) {
			println!("{} is not in the workspace", self.from);
			return
		}
		let to = Crate { name: self.to.clone(), features: vec![] };
		if !dag.rhs_contains(&to) {
			println!("{} is not a dependency in the workspace", to);
			return
		}

		log::info!("Calculating path from {} to {}", from, to);
		match dag.any_path(&from, &to) {
			Some(path) => println!("{path}"),
			None => {
				let forward = dag.node(from.clone());
				let depends = forward.into_transitive_hull_in(&dag);

				if depends.connected(&from, &to) {
					unreachable!(
						"Sanity check failed: {} depends on {} but no path could be found",
						from, to
					);
				}
			},
		}*/
	}

	fn tree(&self, package: &str) -> Dag<Crate> {
		// Spawn cargo tree
		let mut cmd = std::process::Command::new("cargo");
		cmd.arg("tree")
			.arg("--manifest-path")
			.arg(&self.manifest_path)
			.args(["--edges", "features,normal"])
			.args(["--prefix", "depth"])
			.args(["-p", package]);
		if self.workspace {
			cmd.arg("--workspace");
		}
		log::info!("Running {:?}", cmd);
		let output = cmd.output().expect("Failed to run cargo tree");
		if !output.status.success() {
			panic!("cargo tree failed: {}", String::from_utf8_lossy(&output.stderr));
		}
		let output = String::from_utf8(output.stdout).expect("cargo tree output was not utf8");
		let mut lines = output.lines().collect::<Vec<_>>();
		let mut parser = TreeParser::from_lines(lines);
		parser.parse_tree();
		
		Dag::default()
	}
}

use regex::*;

struct TreeParser<'a> {
	stack: Vec<Crate>,
	depth: usize,
	line: &'a str,
	lines: Vec<&'a str>,
}

impl<'a> TreeParser<'a> {
	fn from_lines(lines: Vec<&'a str>) -> Self {
		Self { stack: vec![], depth: 0, line: "", lines }
	}

	fn parse_tree(&mut self) {
		if !self.advance_line() {
			return;
		}

		let mut dag = Dag::<Crate>::default();
		let mut stack = Vec::<Crate>::new();
		
		while !self.lines.is_empty() {
			if !self.advance_line() {
				return;
			}

			let (krate, already_known) = self.parse_crate_def();
			if already_known {
				assert!(dag.lhs_contains(&krate), "Crate should have been known: {}", krate);
			}
						
			match stack.last() {
				Some(parent) => dag.add_edge(parent.clone(), krate.clone()),
				None => (),
			}
			stack.push(krate);
			stack.truncate(self.depth+1);
		}
		log::info!("DAG has {} edges and {} nodes", dag.num_edges(), dag.num_nodes());
	}

	fn parse_crate_def(&mut self) -> (Crate, bool) {
		let re = Regex::new(r"^([^ ]+) (.*)$").expect("Static regex is good");
		let Some(caps) = re.captures(self.line) else {
			panic!("cargo tree output was not in the expected format: {:?}", self.line);
		};
		let name = caps.get(1).unwrap().as_str().trim();
		let rest = caps.get(2).unwrap().as_str().trim();
		let (features, version) = if rest.starts_with("feature") {
			let re = Regex::new("^feature \"([^\"]*)\"").expect("Static regex is good");
			let Some(caps) = re.captures(rest) else {
				panic!("cargo tree output was not in the expected format: {:?}", self.line);
			};
			let features = caps.get(1).unwrap().as_str().trim();
			(Some(features.split(',').map(|f| f.trim().to_string()).collect()), None)
		} else if rest.starts_with("v") {
			let re = Regex::new(r"^v([^ ]+)").expect("Static regex is good");
			let Some(caps) = re.captures(rest) else {
				panic!("cargo tree output was not in the expected format: {:?}", self.line);
			};
			let version = caps.get(1).unwrap().as_str().trim();
			(None, Some(version.to_string()))
		} else {
			panic!("cargo tree output was not in the expected format: {:?}", self.line);
		};
		let known = self.line.ends_with("(*)");
		log::info!("Parsed crate {} {:?} {:?} known={}", name, version, features, known);
		if version == Some("(proc-macro)".into()) {
			panic!("wtf: {:?}", self.line);
		}
		(Crate { name: name.to_string(), version: version.unwrap_or_default(), features: features.unwrap_or_default() }, known)
	}

	fn advance_line(&mut self) -> bool {
		while !self.lines.is_empty() {
			let line = self.lines.remove(0).trim();
			if !line.is_empty() {
				let re = Regex::new("^([0-9]+)").unwrap();
				let Some(caps) = re.captures(line) else {
					panic!("cargo tree output was not in the expected format: {:?}", self.line);
				};
				let cap = caps.get(1).unwrap().as_str();
				self.depth = cap.parse().unwrap();
				self.line = line[cap.len()..].trim();
				return true;
			}
		}
		false
	}
}
