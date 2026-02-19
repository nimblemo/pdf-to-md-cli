use anyhow::{Context, Result};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::logger::set_logger;

/// Entry point for processing: handles single file or directory.
pub fn run(
    input: &Path,
    output_dir: Option<&Path>,
    output_name: Option<&str>,
    stdout: bool,
    verbose: bool,
    log_file: Option<&Path>,
) -> Result<()> {
    if let Some(path) = log_file {
        let file = std::fs::File::create(path)
            .with_context(|| format!("Failed to create log file: {}", path.display()))?;
        set_logger(file);
    }

    let files = collect_pdf_files(input)?;

    if files.is_empty() {
        crate::logger!("No PDF files found in: {}", input.display());
        return Ok(());
    }

    if verbose {
        crate::logger!(
            "Processing {} PDF file(s) using {} threads...",
            files.len(),
            rayon::current_num_threads()
        );
    }

    // Parallel processing with rayon across all CPU cores
    let results: Vec<Result<()>> = files
        .par_iter()
        .map(|file_path| {
            process_single_file(
                file_path,
                output_dir,
                output_name,
                stdout,
                files.len(),
                verbose,
            )
        })
        .collect();

    // Report errors
    let mut had_error = false;
    for result in results {
        if let Err(e) = result {
            crate::logger!("Error: {:#}", e);
            had_error = true;
        }
    }

    if had_error {
        std::process::exit(1);
    }

    Ok(())
}

/// Collect all .pdf files from the given path (file or directory).
fn collect_pdf_files(input: &Path) -> Result<Vec<PathBuf>> {
    if !input.exists() {
        anyhow::bail!("Input path does not exist: {}", input.display());
    }

    if input.is_file() {
        if input
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("pdf"))
        {
            return Ok(vec![input.to_path_buf()]);
        } else {
            return Ok(vec![]);
        }
    }

    let mut pdf_files = Vec::new();
    for entry in WalkDir::new(input).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .map_or(false, |ext| ext.eq_ignore_ascii_case("pdf"))
        {
            pdf_files.push(path.to_path_buf());
        }
    }
    Ok(pdf_files)
}

/// Process a single PDF file: convert and either write to file or print to stdout.
fn process_single_file(
    input_path: &Path,
    output_dir: Option<&Path>,
    output_name: Option<&str>,
    stdout: bool,
    total_files: usize,
    verbose: bool,
) -> Result<()> {
    let start = std::time::Instant::now();

    // Only print progress if processing multiple files or verbose
    if verbose || total_files > 1 {
        crate::logger!("Converting: {}", input_path.display());
    }

    let markdown = crate::converter::convert_file(input_path, verbose)
        .with_context(|| format!("Failed to convert {}", input_path.display()))?;

    if stdout {
        // For multiple files, add a header separator
        if total_files > 1 {
            println!("\n<!-- FILE: {} -->\n", input_path.display());
        }
        println!("{}", markdown);
        return Ok(());
    }

    // Determine output path
    let file_stem = input_path.file_stem().unwrap_or_default();
    let name = output_name
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(file_stem));

    let out_dir = output_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| input_path.parent().unwrap_or(Path::new(".")).to_path_buf());

    std::fs::create_dir_all(&out_dir)?;
    let output_path = out_dir.join(name).with_extension("md");

    std::fs::write(&output_path, markdown)
        .with_context(|| format!("Failed to write to {}", output_path.display()))?;

    if verbose || total_files > 1 {
        let duration = start.elapsed();
        crate::logger!("Finished: {} in {:.2?}", output_path.display(), duration);
    } else {
        // Single file quiet mode
        crate::logger!("Created: {}", output_path.display());
    }

    Ok(())
}
