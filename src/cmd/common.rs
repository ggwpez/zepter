use regex::Regex;
use clap::Parser;
use crate::Crate;
use crate::dag::Dag;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct TreeArgs {
	/// Cargo manifest path.
	#[arg(long, global = true, default_value = "Cargo.toml")]
	pub manifest_path: PathBuf,

	/// Whether to only consider workspace crates.
	#[clap(long, global = true, default_value = "false")]
	pub workspace: bool,

	#[clap(long, global = true)]
	pub features: Vec<String>,
}
/*
pub(crate) fn tree(args: &TreeArgs) -> Dag<Crate> {
	log::info!("Using manifest {:?}", args.manifest_path);
	let mut cmd = std::process::Command::new("cargo");
	cmd.arg("tree")
		.arg("--manifest-path")
		.arg(&args.manifest_path)
		.args(["--edges", "features,normal"])
		.args(["--format", "{p} {f}"])
		.args(["--prefix", "depth"]);
	if args.workspace {
		cmd.arg("--workspace");
	}
	if !args.features.is_empty() {
		cmd.arg("--features").arg(args.features.join(","));
	}
	log::debug!("Running {:?}", cmd);
	let output = cmd.output().expect("Failed to run cargo tree");
	if !output.status.success() {
		panic!("cargo tree failed: {}", String::from_utf8_lossy(&output.stderr));
	}
	let output = String::from_utf8(output.stdout).expect("cargo tree output was not utf8");
	let lines = output.lines().collect::<Vec<_>>();
	let mut parser = TreeParser::from_lines(lines);
	parser.parse_tree()
}

struct TreeParser<'a> {
	depth: usize,
	line: &'a str,
	lines: Vec<&'a str>,
}

impl<'a> TreeParser<'a> {
	fn from_lines(lines: Vec<&'a str>) -> Self {
		Self { depth: 0, line: "", lines }
	}

	fn parse_tree(&mut self) -> Dag<Crate> {
		let mut dag = Dag::<Crate>::default();
		let mut stack = Vec::<Crate>::new();
		
		while !self.lines.is_empty() {
			if !self.advance_line() {
				break;
			}
			stack.truncate(self.depth);

			let (krate, _already_known) = self.parse_crate_def();
						
			dag.add_node(krate.clone());
			match stack.last() {
				Some(parent) => dag.add_edge(parent.clone(), krate.clone()),
				None => (),
			}
			stack.push(krate.clone());
			
		}
		log::debug!("DAG has {} nodes and {} edges", dag.num_nodes(), dag.num_edges());
		dag
	}

	fn parse_crate_def(&mut self) -> (Crate, bool) {
		let re = Regex::new(r"^([^ ]+) (.*)$").expect("Static regex is good");
		let Some(caps) = re.captures(self.line) else {
			panic!("cargo tree output was not in the expected format: {:?}", self.line);
		};
		let name = caps.get(1).unwrap().as_str().trim();
		let rest = caps.get(2).unwrap().as_str().trim();
		let (has_features, enabled_features, version) = if rest.starts_with("feature") {
			let re = Regex::new("^feature \"([^\"]*)\"").expect("Static regex is good");
			let Some(caps) = re.captures(rest) else {
				panic!("cargo tree output was not in the expected format: {:?}", self.line);
			};
			let features = caps.get(1).unwrap().as_str().trim();
			(None, Some(features.split(',').map(|f| f.trim().to_string()).collect()), None)
		} else if rest.starts_with("v") {
			let re = Regex::new(r"^v([^ ]+)").expect("Static regex is good");
			let Some(caps) = re.captures(rest) else {
				panic!("cargo tree output was not in the expected format: {:?}", self.line);
			};
			let version = caps.get(1).unwrap().as_str().trim();
			let mut rest = rest[caps.get(0).unwrap().end()..].trim();
			// remove (*)
			rest = rest.trim_end_matches(" (*)");
			let features = if !rest.ends_with(")") {
				rest.chars().rev().take_while(|c| !c.is_whitespace()).collect::<String>()
				.split(',').map(|f| f.trim().to_string()).map(|f| f.chars().rev().collect::<String>()).collect()
			} else {
				vec![]
			};
			(Some(features), None, Some(version.to_string()))
		} else {
			panic!("cargo tree output was not in the expected format: {:?}", self.line);
		};
		// Sanity check
		for feature in has_features.iter() {
			if feature.contains(&"(".into()) || feature.contains(&")".into()) {
				panic!("fml");
			}
		}
		let known = self.line.ends_with("(*)");
		let krate = Crate { name: name.trim().to_string(), version: version.unwrap_or_default(), enabled_features: enabled_features.unwrap_or_default(), has_features: has_features.unwrap_or_default() };
		(krate, known)
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
*/
