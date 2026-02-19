mod converter;
mod models;
mod processor;
mod transformations;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// pdf-to-md — быстрый конвертер PDF в Markdown с параллельной обработкой
#[derive(Parser, Debug)]
#[command(
    name = "pdf-to-md",
    version,
    about = "Converts PDF files to Markdown format",
    long_about = "A fast, parallel PDF to Markdown converter.\n\
                  Processes single files or entire directories using all available CPU cores.\n\
                  Inspired by the pdf-to-markdown JS project."
)]
struct Cli {
    /// Path to a PDF file or a directory containing PDF files
    #[arg(value_name = "INPUT")]
    input: PathBuf,

    /// Output directory for generated .md files (default: current directory)
    #[arg(short = 'o', long = "output", value_name = "DIR")]
    output: Option<PathBuf>,

    /// Output filename without extension (only for single file input)
    #[arg(short = 'n', long = "name", value_name = "NAME")]
    name: Option<String>,

    /// Print result to stdout instead of writing files
    #[arg(short = 's', long = "stdout")]
    stdout: bool,

    /// Print debug information
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Validate --name only makes sense for a single file
    if cli.name.is_some() && cli.input.is_dir() {
        anyhow::bail!("--name can only be used when INPUT is a single file, not a directory");
    }

    // Validate --name and --stdout are mutually exclusive
    if cli.name.is_some() && cli.stdout {
        anyhow::bail!("--name and --stdout cannot be used together");
    }

    processor::run(
        &cli.input,
        cli.output.as_deref(),
        cli.name.as_deref(),
        cli.stdout,
        cli.verbose,
    )?;

    Ok(())
}
