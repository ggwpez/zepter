// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::GlobalArgs;
use crate::{
	config::{workflow::WORKFLOW_DEFAULT_NAME, ConfigArgs},
	log,
};

#[derive(Default, Debug, clap::Parser)]
pub struct RunCmd {
	#[clap(flatten)]
	pub args: RunArgs,
}

#[derive(Default, Debug, clap::Parser)]
pub struct RunArgs {
	#[clap(flatten)]
	pub config: ConfigArgs,

	#[clap(name = "WORKFLOW", index = 1)]
	pub workflow: Option<String>,
}

impl RunCmd {
	pub fn run(&self, g: &GlobalArgs) {
		let config = self.args.config.load().expect("Invalid config file");

		let name = self.args.workflow.as_deref().unwrap_or(WORKFLOW_DEFAULT_NAME);
		let Some(workflow) = config.workflow(name) else {
			panic!("Workflow '{name}' not found");
		};

		log::info!("Running workflow '{}'", name);
		if let Err(err) = workflow.run(g) {
			println!("Error: {err}");

			if let Some(help) = config.fmt_help() {
				println!("\n{help}");
			}

			std::process::exit(1);
		}
	}
}
