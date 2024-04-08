use std::collections::BTreeMap;
use crate::cmd::CargoArgs;
use crate::cmd::GlobalArgs;
use cargo_metadata::{DependencyKind, Package};
use crate::cmd::resolve_dep;

#[derive(Debug, clap::Parser)]
pub struct NoStdCmd {
	#[clap(subcommand)]
	sub: NoStdSubCmd,
}

#[derive(Debug, clap::Subcommand)]
pub enum NoStdSubCmd {
	/// Default features of no-std dependencies are disabled if the crate itself supports no-std.
	#[clap(name = "default-features-of-nostd-dependencies-disabled")]
	DefaultFeaturesDisabled(DefaultFeaturesDisabledCmd),
}

#[derive(Debug, clap::Parser)]
pub struct DefaultFeaturesDisabledCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	cargo_args: CargoArgs,
}

impl NoStdCmd {
	pub(crate) fn run(&self, global: &GlobalArgs) -> Result<(), String> {
		match &self.sub {
			NoStdSubCmd::DefaultFeaturesDisabled(cmd) => cmd.run(global),
		}
	}
}

impl DefaultFeaturesDisabledCmd {
	pub(crate) fn run(&self, _: &GlobalArgs) -> Result<(), String> {
		let meta = self.cargo_args.clone().with_workspace(true).load_metadata()?;
		//let dag = build_feature_dag(&meta, &meta.packages);
		let pkgs = &meta.packages;
		let mut cache = BTreeMap::new();

		for lhs in pkgs.iter() {
			// check if lhs supports no-std builds
			if !Self::supports_nostd(lhs, &mut cache)? {
				continue;
			}

			for dep in lhs.dependencies.iter() {
				if dep.kind != DependencyKind::Normal {
					continue;
				}

				let Some(rhs) = resolve_dep(lhs, dep, &meta) else {
					continue
				};

				if !Self::supports_nostd(&rhs.pkg, &mut cache)? {
					continue;
				}

				if dep.uses_default_features {
					log::warn!("{} depends on {} with default features", lhs.name, rhs.pkg.name);
				}
			}
		}

		Ok(())
	}

	fn supports_nostd(krate: &Package, cache: &mut BTreeMap<String, bool>) -> Result<bool, String> {
		log::debug!("Checking if crate supports no-std: {}", krate.name);
		if let Some(res) = cache.get(krate.manifest_path.as_str()) {
			return Ok(*res)
		}

		// try to find the lib.rs
		let krate_root = krate.manifest_path.parent().ok_or_else(|| format!("Could not find parent of manifest: {}", krate.manifest_path))?;
		let lib_rs = krate_root.join("src/lib.rs");

		if !lib_rs.exists() {
			return Ok(false)
		}
		let content = std::fs::read_to_string(&lib_rs).map_err(|e| format!("Could not read lib.rs: {}", e))?;

		let ret = if content.contains("#![cfg_attr(not(feature = \"std\"), no_std)]") || content.contains("#![no_std]") {
			log::debug!("Crate supports no-std: {} (path={})", krate.name, krate.manifest_path);
			true
		} else {
			false
		};

		cache.insert(krate.manifest_path.as_str().into(), ret);
		Ok(ret)
	}
}
