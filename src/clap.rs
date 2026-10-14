//! Command-line arguments for `dev` builds, parsed into [`AppSettings`].

use std::process::exit;

use clap::Parser;
use quell::{AppSettings, prelude::*};

/// Build metadata written by `build.rs`.
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// The `dev` command line. Run the binary with `--help` for usage.
#[derive(Parser, Debug)]
struct Cli {
    /// Name of the screen to start on
    #[arg(short, long)]
    screen: Option<String>,

    /// Show build info
    #[arg(long)]
    build_info: bool,

    /// Passed by the Jackdaw editor when extracting the type schema; handled
    /// by `jackdaw_runtime`, so only accept it here.
    #[arg(long = "jackdaw-extract-schema", hide = true)]
    _jackdaw_extract_schema: bool,
}

/// Parses the command line into [`AppSettings`].
///
/// Exits the process after printing when `--build-info` is passed.
pub fn parse_args() -> AppSettings {
    let args = Cli::parse();

    if args.build_info {
        println!("Build timestamp: {}", built_info::BUILT_TIME_UTC);
        exit(0);
    }

    AppSettings {
        initial_screen: args.screen,
    }
}
