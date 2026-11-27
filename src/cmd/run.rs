// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: Oliver Tale-Yazdi <oliver@tasty.limo>

use super::{CommandError, GlobalArgs};
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
	pub fn run(&self, g: &GlobalArgs) -> Result<(), CommandError> {
		let config = self.args.config.load().map_err(CommandError::InvalidConfig)?;

		let name = self.args.workflow.as_deref().unwrap_or(WORKFLOW_DEFAULT_NAME);
		let workflow = config
			.workflow(name)
			.ok_or_else(|| CommandError::InvalidConfig(format!("Workflow '{name}' not found")))?;

		log::info!("Running workflow '{}'", name);
		if let Err(err) = workflow.run(g) {
			println!("Error: {err}");

			if let Some(help) = config.fmt_help() {
				println!("\n{help}");
			}

			return Err(CommandError::reported(1))
		}
		Ok(())
	}
}
