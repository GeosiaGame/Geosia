use clap::Parser;

#[derive(Parser)]
#[command(name = "ocg_dedi_server", about = "OpenCubeGame dedicated server")]
struct CliOptions {}

fn main() {
    let _cli = CliOptions::parse();
}
