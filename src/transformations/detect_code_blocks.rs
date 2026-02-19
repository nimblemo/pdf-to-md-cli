use crate::models::{BlockType, ParseResult, WordFormat};
use crate::transformations::common::Transformation;

pub struct DetectCodeBlocks {
    pub verbose: bool,
}

impl Transformation for DetectCodeBlocks {
    fn transform(&self, result: &mut ParseResult) {
        let globals = &result.globals;
        let _most_used_distance = globals.most_used_distance;
        let total_pages = result.pages.len();

        for (i, page) in result.pages.iter_mut().enumerate() {
            if self.verbose {
                eprintln!("DetectCodeBlocks: Processed {}/{} pages...", i + 1, total_pages);
            }

            // Calculate min_x for the page to determine indentation
            let mut min_x = f64::MAX;
            for item in &page.items {
                if let crate::models::ItemType::LineItem(line) = item {
                    if line.x < min_x {
                        min_x = line.x;
                    }
                }
            }
            if min_x == f64::MAX {
                min_x = 0.0;
            }

            // Increase threshold to avoid capturing slightly indented paragraphs/quotes
            // user feedback: "Tanya D'cruz" matching indentation logic.
            // Using a smaller threshold to match JS behavior (x > minX)
            // But keeping a small buffer for float precision.
            let indent_threshold = min_x + 2.0;

            // Collect groups of consecutive italics
            let mut groups = Vec::new();
            let mut current_group = Vec::new();

            for (idx, item) in page.items.iter_mut().enumerate() {
                if let crate::models::ItemType::LineItem(line) = item {
                    let text = line.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>().join("");
                    let is_formatted = line.items.iter().all(|i| i.format.is_some());
                    
                    // Check for manual bold formatting (e.g. "**Preface**")
                    let has_manual_bold = text.trim().starts_with("**") && text.trim().ends_with("**");

                    if text.contains("Preface") || text.contains("My special thanks") {
                         if self.verbose {
                             eprintln!("DEBUG: Text='{}', is_formatted={}, has_manual_bold={}, x={}, threshold={}", 
                                 text, is_formatted, has_manual_bold, line.x, indent_threshold);
                         }
                    }

                    if line.block_type == BlockType::Paragraph {
                        // Check indentation
                        if line.x > indent_threshold {
                            // Check height: Single lines that are tall are likely Headers/Titles, not Code.
                            // e.g. "David N. Blank-Edelman" (21.0) vs most_used (10.5)
                            if line.height > globals.most_used_height + 1.0 {
                                continue;
                            }

                            // Secondary check: Is it likely code?
                            // Code usually isn't all Bold or all Italic.
                            // "Contributors" was **Contributors** and indented -> false positive.
                            
                            // Also check if text contains letters? Code usually does, but so does text.
                            // Monospace check would be ideal but we don't have is_mono yet.
                            // "Preface" case: **Preface** is bold but might not have format property set if text has **.
                            
                            if !is_formatted && !has_manual_bold {
                                if self.verbose && (text.contains("Preface") || text.contains("My special thanks")) {
                                    eprintln!("DEBUG: Setting Code for '{}' due to indentation", text);
                                }
                                line.block_type = BlockType::Code;
                            }
                        }
                    }
                    
                    // Italic Check for User Rule: "if multiple lines in a row are highlighted in italics -> code block"
                    // Check if items are Italic format OR text is wrapped in underscores/asterisks
                    // AND it is not a header (by height or specific keyword)
                    let is_likely_header = line.height > globals.most_used_height + 1.0 || text.contains("Preface");
                    
                    let is_italic = !is_likely_header && (
                        line.items.iter().all(|i| matches!(i.format, Some(WordFormat::Italic) | Some(WordFormat::BoldItalic)))
                        || (text.trim().starts_with('_') && text.trim().ends_with('_'))
                        || (text.trim().starts_with('*') && text.trim().ends_with('*') && !has_manual_bold)
                    );

                    if is_italic {
                        current_group.push(idx);
                    } else {
                        // Non-italic line breaks the group
                        if current_group.len() > 1 {
                             groups.push(current_group.clone());
                        }
                        current_group.clear();
                    }
                }
                // Non-LineItems (images, etc) are ignored and do NOT break the group
            }
            
            // Process last group
            if current_group.len() > 1 {
                groups.push(current_group);
            }

            // Apply groups
            for group in groups {
                for idx in group {
                    if let crate::models::ItemType::LineItem(line) = &mut page.items[idx] {
                         if self.verbose {
                             let text = line.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>().join("");
                             if text.contains("My special thanks") {
                                 eprintln!("DEBUG: Setting Code for '{}' due to italics group", text);
                             }
                         }
                         line.block_type = BlockType::Code;
                    }
                }
            }

            // Post-process: Normalize indentation for all Code blocks
            let mut code_block_start = None;
            let mut current_block_indices = Vec::new();

            for idx in 0..page.items.len() {
                let is_code = if let crate::models::ItemType::LineItem(line) = &page.items[idx] {
                    line.block_type == BlockType::Code
                } else {
                    false
                };

                if is_code {
                    if code_block_start.is_none() {
                        code_block_start = Some(idx);
                    }
                    current_block_indices.push(idx);
                } else {
                    if !current_block_indices.is_empty() {
                        // Process the finished block
                        Self::normalize_indentation(&mut page.items, &current_block_indices);
                        current_block_indices.clear();
                    }
                    code_block_start = None;
                }
            }
            // Process last block
            if !current_block_indices.is_empty() {
                Self::normalize_indentation(&mut page.items, &current_block_indices);
            }
        }
    }
}

impl DetectCodeBlocks {
    fn normalize_indentation(items: &mut [crate::models::ItemType], indices: &[usize]) {
        if indices.is_empty() {
            return;
        }

        // Find min_x for the block
        let mut min_x = f64::MAX;
        for &idx in indices {
            if let crate::models::ItemType::LineItem(line) = &items[idx] {
                if line.x < min_x {
                    min_x = line.x;
                }
            }
        }

        if min_x == f64::MAX {
            return;
        }

        // Apply relative indentation
        for &idx in indices {
            if let crate::models::ItemType::LineItem(line) = &mut items[idx] {
                let delta = line.x - min_x;
                // Heuristic: 1 space approx 4.0 units (depends on font size, usually 10pt -> char width ~5-6)
                // Let's assume 5.0 units per space for safety?
                // Or 4.0?
                // If delta is small (jitter), ignore.
                if delta > 2.0 {
                    let spaces = (delta / 4.0).round() as usize;
                    if spaces > 0 {
                        let prefix = " ".repeat(spaces);
                        if let Some(first_item) = line.items.first_mut() {
                            first_item.text.insert_str(0, &prefix);
                        }
                    }
                }
            }
        }
    }
}
