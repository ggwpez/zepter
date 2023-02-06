use clap::Parser;
use feature::cmd::Command;

fn main() {
	let cmd = Command::parse();
	env_logger::Builder::from_env(env_logger::Env::default()).init();
	cmd.run();
}
