use feature::cmd::Command;
use clap::Parser;

fn main() {
	let cmd = Command::parse();
	env_logger::Builder::from_env(env_logger::Env::default()).init();
	cmd.run();
}
