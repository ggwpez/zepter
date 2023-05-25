// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use assert_cmd::{assert::OutputAssertExt, Command};
use feature::mock::*;
use std::{
	collections::HashMap,
	fs,
	io::Write,
	path::{Path, PathBuf},
};

pub type ModuleName = String;
pub type FeatureName = String;

pub struct Context {
	root: tempfile::TempDir,
}

impl Context {
	fn new() -> Self {
		Self { root: tempfile::tempdir().unwrap() }
	}

	fn persist(self) -> PathBuf {
		self.root.into_path()
	}

	fn create_crate(&self, module: &CrateConfig) -> Result<(), anyhow::Error> {
		self.cargo(&format!("new --vcs=none --offline --lib {}", module.name), None)?;
		let toml_path = self.root.path().join(&module.name).join("Cargo.toml");
		assert!(toml_path.exists(), "Crate must exist");
		// Add the deps
		let mut out_deps = String::from("");
		for dep in module.deps.iter().into_iter().flatten() {
			out_deps.push_str(&dep.def());
		}

		let mut txt = String::from("[features]\n");
		for (feature, enables) in module.features.iter().into_iter().flatten() {
			txt.push_str(&format!("{} = [\n", feature));
			for (dep, feat) in enables.iter().into_iter().flatten() {
				txt.push_str(&format!("\"{}/{}\",\n", dep, feat));
			}
			txt.push_str("]\n");
		}

		let output = format!("{}\n{}", out_deps, txt);
		// Append to the toml
		let mut file = fs::OpenOptions::new().append(true).open(toml_path).unwrap();
		file.write_all(output.as_bytes()).unwrap();
		Ok(())
	}

	fn create_workspace(&self, subs: &[CrateConfig]) -> Result<(), anyhow::Error> {
		let mut txt = String::from("[workspace]\nmembers = [");
		for sub in subs.iter() {
			txt.push_str(&format!("\"{}\",", sub.name));
		}
		txt.push_str("]");
		let toml_path = self.root.path().join("Cargo.toml");
		fs::write(toml_path, txt)?;
		Ok(())
	}

	fn cargo(&self, cmd: &str, sub_dir: Option<&str>) -> Result<(), anyhow::Error> {
		assert!(self.root.path().exists());
		let dir = match sub_dir {
			Some(sub_dir) => self.root.path().join(sub_dir),
			None => self.root.path().to_owned(),
		};

		let args = cmd.split_whitespace().collect::<Vec<_>>();
		let output = Command::new("cargo")
			.args(&args)
			.current_dir(&dir)
			.output()
			.expect("failed to execute cargo");

		if !output.status.success() {
			Err(anyhow::Error::msg(String::from_utf8(output.stderr).unwrap()))
		} else {
			Ok(())
		}
	}
}

#[test]
fn ui() {
	let filter = std::env::var("UI_FILTER").unwrap_or_else(|_| "**/*.yaml".into());
	let regex = format!("tests/ui/{}", filter);
	// Loop through all files in tests/ recursively
	let files = glob::glob(&regex).unwrap();
	let overwrite = std::env::var("OVERWRITE").is_ok();
	let (mut failed, mut good) = (0, 0);

	// Update each time you add a test.
	for file in files.filter_map(Result::ok) {
		let mut config = CaseFile::from_file(&file);
		let workspace = config.init();
		let mut overwrites = HashMap::new();
		let mut diff_overwrites = HashMap::new();
		let m = config.cases.len();

		for (i, case) in config.cases.iter().enumerate() {
			// pad with spaces to len 30
			colour::white!("Testing {} {}/{} .. ", file.display(), i + 1, m);
			let mut cmd = Command::cargo_bin("feature").unwrap();
			for arg in case.cmd.split_whitespace() {
				cmd.arg(arg);
			}
			cmd.args(&["--manifest-path", workspace.root.path().to_str().unwrap()]);
			cmd.arg("--offline");

			// remove empty trailing and suffix lines
			let res = cmd.output().unwrap();
			if let Some(code) = case.code {
				res.clone().assert().code(code);
			} else {
				res.clone().assert().success();
			}

			match res.stdout == case.stdout.as_bytes() {
				true => {
					colour::green_ln!("OK");
					colour::white!("");
					good += 1;
				},
				false if !overwrite => {
					colour::red_ln!("FAILED");
					pretty_assertions::assert_eq!(
						&String::from_utf8_lossy(&res.stdout),
						&normalize(&case.stdout)
					);
				},
				false => {
					colour::yellow_ln!("OVERWRITE");
					colour::white!("");
					overwrites.insert(i, String::from_utf8_lossy(&res.stdout).to_string());
					failed += 1;
				},
			}

			let got = git_diff(&workspace.root.path()).unwrap();
			if got != case.diff {
				if std::env::var("OVERWRITE").is_ok() {
					diff_overwrites.insert(i, got);
					colour::yellow_ln!("OVERWRITE");
					colour::white!("");
				} else {
					colour::red_ln!("FAILED");
					colour::white!("");
					pretty_assertions::assert_eq!(got, case.diff);
				}
			} else {
				colour::green_ln!("OK");
				colour::white!("");
			}
			git_reset(&workspace.root.path()).unwrap();
		}

		if std::env::var("PERSIST").is_ok() {
			let path = workspace.persist();
			println!("Persisted to {:?}", path);
		}

		if std::env::var("OVERWRITE").is_ok() {
			if overwrites.is_empty() && diff_overwrites.is_empty() {
				continue
			}

			for (i, stdout) in overwrites.into_iter() {
				config.cases[i].stdout = stdout;
			}
			for (i, diff) in diff_overwrites.into_iter() {
				config.cases[i].diff = diff;
			}

			let mut fd = fs::File::create(&file).unwrap();
			serde_yaml::to_writer(&mut fd, &config).unwrap();
			println!("Updated {}", file.display());
		}
	}

	if failed == 0 && good == 0 {
		panic!("No tests found");
	}
}

impl CaseFile {
	pub fn init(&self) -> Context {
		let ctx = Context::new();
		for module in self.crates.iter() {
			ctx.create_crate(&module).unwrap();
		}
		ctx.create_workspace(&self.crates).unwrap();
		git_init(&ctx.root.path()).unwrap();
		ctx
	}
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CaseFile {
	pub crates: Vec<CrateConfig>,
	pub cases: Vec<Case>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CrateConfig {
	name: ModuleName,
	#[serde(skip_serializing_if = "Option::is_none")]
	deps: Option<Vec<Dependency>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	features: Option<HashMap<String, Option<Vec<(String, String)>>>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(untagged)]
pub enum Dependency {
	Normal(String),
	Renamed { name: String, rename: String },
}

impl CaseFile {
	pub fn from_file(path: &Path) -> Self {
		let content = fs::read_to_string(path).unwrap();
		let content = content.replace("\t", "  ");
		serde_yaml::from_str(&content).expect(&format!("Failed to parse: {}", &path.display()))
	}
}

impl Dependency {
	fn def(&self) -> String {
		let mut ret = match &self {
			Self::Renamed { name, rename } => format!("{} = {{ package = \"{}\", ", rename, name),
			Self::Normal(name) => format!("{} = {{ ", name),
		};
		ret.push_str(&format!("version = \"*\", path = \"../{}\" }}\n", self.name()));
		ret
	}

	fn name(&self) -> String {
		match self {
			Self::Renamed { name, .. } | Self::Normal(name) => name.clone(),
		}
	}
}
