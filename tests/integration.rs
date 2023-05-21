// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use assert_cmd::Command;
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
		self.cargo(&format!("new --lib {}", module.name), None)?;
		let toml_path = self.root.path().join(&module.name).join("Cargo.toml");
		assert!(toml_path.exists(), "Crate must exist");
		// Add the deps
		let mut out_deps = String::from("");
		for dep in module.deps.iter().into_iter().flatten() {
			out_deps
				.push_str(&format!("{} = {{ version = \"*\", path = \"../{}\" }}\n", &dep, &dep));
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
fn integration_test() {
	let modules = vec![
		CrateConfig {
			name: "mod0".into(),
			deps: None,
			features: Some(HashMap::from([("f0".into(), None)])),
		},
		CrateConfig {
			name: "mod1".into(),
			deps: Some(vec!["mod0".into()]),
			features: Some(HashMap::from([(
				"f0".into(),
				Some(vec![("mod0".into(), "f0".into())]),
			)])),
		},
		CrateConfig {
			name: "mod2".into(),
			deps: Some(vec!["mod0".into(), "mod1".into()]),
			features: None,
		},
	];

	let ctx = Context::new();
	for module in modules.iter() {
		ctx.create_crate(&module).unwrap();
	}
	ctx.create_workspace(&modules).unwrap();
	let check = ctx.cargo("check", None);

	if std::env::var("PERSIST").is_ok() {
		let path = ctx.persist();
		println!("Persisted to {:?}", path);
	}
	check.unwrap();
}

#[test]
fn ui() {
	let filter = std::env::var("UI_FILTER").unwrap_or_else(|_| "**".into());
	let regex = format!("tests/ui/{}/*.yaml", filter);
	// Loop through all files in tests/ recursively
	let files = glob::glob(&regex).unwrap();
	let overwrite = std::env::var("OVERWRITE").is_ok();
	let mut failed = 0;

	// Update each time you add a test.
	for file in files.filter_map(Result::ok) {
		let mut config = CaseFile::from_file(&file);
		let workspace = config.init();
		let mut overwrites = HashMap::new();

		for (i, case) in config.cases.iter().enumerate() {
			// pad with spaces to len 30
			println!("Testing {}/{}", file.display(), i);
			let mut cmd = Command::cargo_bin("feature").unwrap();
			for arg in case.cmd.split_whitespace() {
				cmd.arg(arg);
			}
			cmd.args(&["--manifest-path", workspace.root.path().to_str().unwrap()]);
			// remove empty trailing and suffix lines
			let res = cmd.output().unwrap();
			match res.stdout == case.stdout.as_bytes() {
				true => {},
				false if !overwrite => {
					pretty_assertions::assert_eq!(
						&String::from_utf8_lossy(&res.stdout),
						&normalize(&case.stdout)
					);
				},
				false => {
					overwrites.insert(i, String::from_utf8_lossy(&res.stdout).to_string());
					failed += 1;
				},
			}
		}

		if std::env::var("OVERWRITE").is_ok() {
			if overwrites.is_empty() {
				continue
			}

			for (i, stdout) in overwrites.into_iter() {
				config.cases[i].stdout = stdout;
			}

			let mut fd = fs::File::create(&file).unwrap();
			serde_yaml::to_writer(&mut fd, &config).unwrap();
			println!("Updated {}", file.display());
		}
	}

	if failed > 0 {
		if std::env::var("OVERWRITE").is_ok() {
			println!("Updated {} test(s)", failed);
		} else {
			panic!("{} test(s) failed", failed);
		}
	}
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Case {
	pub cmd: String,
	pub stdout: String,
}

impl CaseFile {
	pub fn init(&self) -> Context {
		let ctx = Context::new();
		for module in self.crates.iter() {
			ctx.create_crate(&module).unwrap();
		}
		ctx.create_workspace(&self.crates).unwrap();
		//let check = ctx.cargo("check", None);
		//check.unwrap();
		ctx
	}
}

/// Removes leading and trailing empty lines.
fn normalize(s: &str) -> String {
	let mut lines = s.lines().collect::<Vec<_>>();
	while lines.first().map(|l| l.is_empty()).is_some() {
		lines.remove(0);
	}
	while lines.last().map(|l| l.is_empty()).is_some() {
		lines.pop();
	}
	format!("{}\n", lines.join("\n"))
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
	deps: Option<Vec<String>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	features: Option<HashMap<String, Option<Vec<(String, String)>>>>,
}

impl CaseFile {
	pub fn from_file(path: &Path) -> Self {
		let content = fs::read_to_string(path).unwrap();
		let content = content.replace("\t", "  ");
		serde_yaml::from_str(&content).unwrap()
	}
}