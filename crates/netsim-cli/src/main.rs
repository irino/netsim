use clap::Parser;
use netsim_cli::{run, Cli};

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}
