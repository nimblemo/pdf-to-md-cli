use anyhow::Result;
use pdfium_render::prelude::*;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::models::{GlobalStats, ItemType, Page, ParseResult, TextItem};
use crate::transformations::{
    common::Transformation, compact_lines::CompactLines, detect_headers::DetectHeaders,
    stats::CalculateGlobalStats, to_markdown::ToMarkdown,
};

/// Convert a PDF file at `path` to a Markdown string.
pub fn convert_file(path: &Path, verbose: bool) -> Result<String> {
    if verbose {
        eprintln!("Loading PDF from: {}", path.display());
    }

    // Initialize Pdfium in main thread to verify library is present, then drop it.
    {
        let _ = Pdfium::new(
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./lib/"))
                .or_else(|_| {
                    Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
                })
                .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name()))?,
        );
    }

    // Load Document to get page count
    // We create a separate Pdfium instance just to get the page count from the file.
    let total_pages = {
        let pdfium = Pdfium::new(
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./lib/"))
                .or_else(|_| {
                    Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
                })
                .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name()))?,
        );
        let document = pdfium.load_pdf_from_file(path, None)?;
        document.pages().len()
    };

    if verbose {
        eprintln!("Total pages: {}", total_pages);
    }

    // 3. Extract Pages Parallelly
    let num_threads = rayon::current_num_threads();
    let chunk_size = (total_pages as usize + num_threads - 1) / num_threads;

    // Create ranges
    let ranges: Vec<(u16, u16)> = (0..total_pages)
        .step_by(chunk_size)
        .map(|start| {
            let end = std::cmp::min(start + chunk_size as u16, total_pages);
            (start, end)
        })
        .collect();

    if verbose {
        eprintln!(
            "Processing {} pages using {} threads ({} chunks)...",
            total_pages,
            num_threads,
            ranges.len()
        );
    }

    let extraction_counter = AtomicUsize::new(0);

    let mut pages: Vec<Page> = ranges
        .par_iter()
        .map(|&(start, end)| {
            // Each thread creates its own Pdfium instance
            let pdfium = Pdfium::new(
                Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./lib/"))
                    .or_else(|_| {
                        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
                    })
                    .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name()))
                    .expect("Failed to bind Pdfium in thread"),
            );

            let doc = pdfium
                .load_pdf_from_file(path, None)
                .expect("Failed to open PDF in thread");
            let mut chunk_pages = Vec::with_capacity((end - start) as usize);

            // Reuse the guard to keep library loaded?
            // Actually, creating new Pdfium(bindings) calls InitLibrary which increments refcount.
            // So it should be fine.

            for page_idx in start..end {
                if let Ok(page) = doc.pages().get(page_idx) {
                    let items = extract_text_items(&page);
                    chunk_pages.push(Page {
                        index: page_idx,
                        items,
                    });
                }

                // Progress log
                if verbose {
                    let c = extraction_counter.fetch_add(1, Ordering::Relaxed) + 1;
                    if c % 10 == 0 || c == total_pages as usize {
                        eprintln!("Extracted page {}/{}", c, total_pages);
                    }
                }
            }
            chunk_pages
        })
        .flatten()
        .collect();

    // Sort pages by index
    pages.sort_by_key(|p| p.index);

    if verbose {
        eprintln!(
            "Extracted {} pages in total. Calculating global stats...",
            pages.len()
        );
    }

    // 4. Create ParseResult
    let mut result = ParseResult {
        pages,
        globals: GlobalStats::default(),
    };

    // Calculate stats
    CalculateGlobalStats { verbose }.transform(&mut result);

    if verbose {
        eprintln!(
            "Global stats: most_used_height={}, most_used_font='{}', most_used_distance={}",
            result.globals.most_used_height,
            result.globals.most_used_font,
            result.globals.most_used_distance
        );
    }

    // 5. Run Transformation Pipeline

    if verbose {
        eprintln!("Running RemoveRepetitiveElements...");
    }
    use crate::transformations::remove_repetitive_elements::RemoveRepetitiveElements;
    RemoveRepetitiveElements { verbose }.transform(&mut result);

    if verbose {
        eprintln!("Running CompactLines...");
    }
    CompactLines { verbose }.transform(&mut result);

    if verbose {
        eprintln!("Running DetectCodeBlocks...");
    }
    use crate::transformations::detect_code_blocks::DetectCodeBlocks;
    DetectCodeBlocks { verbose }.transform(&mut result);

    if verbose {
        eprintln!("Running DetectTOC...");
    }
    use crate::transformations::detect_toc::DetectTOC;
    DetectTOC { verbose }.transform(&mut result);

    if verbose {
        eprintln!("Running DetectHeaders...");
    }
    DetectHeaders { verbose }.transform(&mut result);

    if verbose {
        eprintln!("Generating Markdown...");
    }
    ToMarkdown { verbose }.transform(&mut result);

    // Combine pages
    let page_markdowns: Vec<String> = result
        .pages
        .iter()
        .filter_map(|p| {
            // Find the markdown item
            p.items.iter().find_map(|item| {
                if let ItemType::Markdown(s) = item {
                    Some(s.clone())
                } else {
                    None
                }
            })
        })
        .collect();

    let mut final_markdown = String::new();

    for (i, page_md) in page_markdowns.iter().enumerate() {
        if i > 0 {
            let prev = &page_markdowns[i - 1];
            let trimmed_prev = prev.trim_end();

            if !trimmed_prev.is_empty() {
                let last_char = trimmed_prev.chars().last().unwrap();
                let is_sentence_end = ".?!\"”’".contains(last_char);

                if is_sentence_end {
                    // Paragraph break needed. Ensure we have at least 2 newlines.
                    if !prev.ends_with("\n\n") {
                        if prev.ends_with('\n') {
                            final_markdown.push('\n');
                        } else {
                            final_markdown.push_str("\n\n");
                        }
                    }
                } else {
                    // Continuation needed (soft wrap). Ensure we have 1 newline.
                    // If prev ends with \n\n, we can't easily join, but headers usually end with \n\n.
                    // If it's a paragraph split, prev likely ends with \n.
                    if !prev.ends_with('\n') {
                        final_markdown.push('\n');
                    }
                    // If prev ends with \n\n, it remains a break.
                    // If prev ends with \n, it remains a soft wrap.
                }
            } else {
                final_markdown.push('\n');
            }
        }
        final_markdown.push_str(page_md);
    }

    Ok(final_markdown)
}

fn extract_text_items(page: &PdfPage) -> Vec<ItemType> {
    let mut items = Vec::new();

    for object in page.objects().iter() {
        if let Some(text_object) = object.as_text_object() {
            let text = text_object.text();
            if text.trim().is_empty() {
                continue;
            }

            let font_name = text_object.font().name();
            let bounds = text_object.bounds().unwrap_or(PdfQuadPoints::zero());

            let width = (bounds.width().value).abs() as f64;
            let height = (bounds.height().value).abs() as f64;

            items.push(ItemType::TextItem(TextItem {
                text,
                x: bounds.left().value as f64,
                y: bounds.top().value as f64,
                width,
                height,
                font: font_name,
                font_size: text_object.scaled_font_size().value as f64,
                format: None,
            }));
        }
    }

    // items.sort_by(|a, b| match (a, b) {
    //     (ItemType::TextItem(ta), ItemType::TextItem(tb)) => {
    //         tb.y.partial_cmp(&ta.y)
    //             .unwrap_or(std::cmp::Ordering::Equal)
    //             .then_with(|| ta.x.partial_cmp(&tb.x).unwrap_or(std::cmp::Ordering::Equal))
    //     }
    //     _ => std::cmp::Ordering::Equal,
    // });

    // Sort logic removed to preserve content stream order (likely reading order)
    // which fixes column interleaving issues.

    items
}
