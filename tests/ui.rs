// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use assert_cmd::{assert::OutputAssertExt, Command};
use std::{
	collections::HashMap,
	fs,
	io::Write,
	path::{Path, PathBuf},
};
use zepter::mock::*;

pub type ModuleName = String;
pub type FeatureName = String;

pub struct Context {
	root: tempfile::TempDir,
}

impl Context {
	fn new() -> Self {
		Self { root: tempfile::tempdir().expect("Must create a temporary directory") }
	}

	fn persist(self) -> PathBuf {
		self.root.into_path()
	}

	fn create_crate(&self, module: &CrateConfig) -> Result<(), anyhow::Error> {
		self.cargo(
			&format!("new --vcs=none --offline --lib --name {} {}", module.name, module.path()),
			None,
		)?;
		let toml_path = self.root.path().join(&module.path()).join("Cargo.toml");
		assert!(toml_path.exists(), "Crate must exist");
		// Add the deps
		let mut out_deps = String::from("");
		for dep in module.deps.iter().flatten() {
			out_deps.push_str(&dep.def());
		}

		let mut txt = String::from("[features]\n");
		for (feature, enables) in module.features.iter().flatten() {
			txt.push_str(&format!("{} = [\n", feature));
			for (dep, feat) in enables.iter().flatten() {
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
			txt.push_str(&format!("\"{}\",", sub.path()));
		}
		txt.push(']');
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
	let keep_going = std::env::var("KEEP_GOING").is_ok();
	let (mut failed, mut good) = (0, 0);

	// Update each time you add a test.
	for file in files.filter_map(Result::ok) {
		let mut config = CaseFile::from_file(&file);
		let workspace = config.init().expect(&format!("Failed to run test case {file:?}"));
		let mut overwrites = HashMap::new();
		let mut diff_overwrites = HashMap::new();
		let m = config.cases.len();

		for (i, case) in config.cases.iter().enumerate() {
			// pad with spaces to len 30
			colour::white!("Testing {} {}/{} .. ", file.display(), i + 1, m);
			let mut cmd = Command::cargo_bin("zepter").unwrap();
			for arg in case.cmd.split_whitespace() {
				cmd.arg(arg);
			}
			cmd.args(["--manifest-path", workspace.root.path().to_str().unwrap()]);
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
					colour::green!("cout:OK");
					colour::white!(" ");
					good += 1;
				},
				false if !overwrite => {
					colour::red!("cout:FAILED");
					if !keep_going {
						pretty_assertions::assert_eq!(
							&String::from_utf8_lossy(&res.stdout),
							&normalize(&case.stdout)
						);
					}
				},
				false => {
					colour::yellow!("cout:OVERWRITE");
					colour::white!(" ");
					overwrites.insert(i, String::from_utf8_lossy(&res.stdout).to_string());
					failed += 1;
				},
			}

			let got = git_diff(workspace.root.path()).unwrap();
			if got != case.diff {
				if std::env::var("OVERWRITE").is_ok() {
					diff_overwrites.insert(i, got);
					colour::yellow_ln!("diff:OVERWRITE");
					colour::white!("");
				} else {
					colour::red_ln!("diff:FAILED");
					colour::white!("");
					if !keep_going {
						pretty_assertions::assert_eq!(got, case.diff);
					}
				}
			} else {
				colour::green_ln!("diff:OK");
				colour::white!("");
			}
			git_reset(workspace.root.path()).unwrap();
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
	pub fn init(&self) -> Result<Context, anyhow::Error> {
		let ctx = Context::new();
		for module in self.crates.iter() {
			ctx.create_crate(module)?;
		}
		ctx.create_workspace(&self.crates)?;
		git_init(ctx.root.path())?;
		Ok(ctx)
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

impl CrateConfig {
	/// Return the file path of this crate.
	pub fn path(&self) -> String {
		crate_name_to_path(&self.name)
	}
}

/// Convert a crate's name to a file path.
///
/// This is needed for case-insensitive file systems like on MacOS. It prefixes all lower-case
/// letters with an `l` and turns the upper case.
pub(crate) fn crate_name_to_path(n: &str) -> String {
	n.chars()
		.map(|c| if c.is_lowercase() { format!("l{}", c.to_uppercase()) } else { c.into() })
		.collect()
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(untagged)]
pub enum Dependency {
	Implicit(String),
	Explicit {
		name: String,
		#[serde(skip_serializing_if = "Option::is_none")]
		rename: Option<String>,
		#[serde(skip_serializing_if = "is_false")]
		optional: Option<bool>,
	},
}

impl CaseFile {
	pub fn from_file(path: &Path) -> Self {
		let content = fs::read_to_string(path).unwrap();
		let content = content.replace('\t', "  ");
		serde_yaml::from_str(&content)
			.unwrap_or_else(|_| panic!("Failed to parse: {}", &path.display()))
	}
}

impl Dependency {
	fn def(&self) -> String {
		let option = if self.optional() { ", optional = true".to_string() } else { String::new() };
		let mut ret = match self.rename() {
			Some(rename) => format!("{} = {{ package = \"{}\", ", rename, self.name()),
			None => format!("{} = {{ ", self.name()),
		};
		ret.push_str(&format!("version = \"*\", path = \"../{}\"{}}}\n", self.path(), option));
		ret
	}

	fn path(&self) -> String {
		crate_name_to_path(&self.name())
	}

	fn name(&self) -> String {
		match self {
			Self::Explicit { name, .. } | Self::Implicit(name) => name.clone(),
		}
	}

	fn rename(&self) -> Option<String> {
		match self {
			Self::Explicit { rename, .. } => rename.clone(),
			_ => None,
		}
	}

	fn optional(&self) -> bool {
		match self {
			Self::Explicit { optional, .. } => optional.unwrap_or_default(),
			_ => false,
		}
	}
}

/// Predicate for serde to skip serialization of default values.
fn is_false(b: &Option<bool>) -> bool {
	!b.unwrap_or_default()
}
