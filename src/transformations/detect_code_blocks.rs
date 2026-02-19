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

        for (_i, page) in result.pages.iter_mut().enumerate() {
            if self.verbose {
                crate::lgger!("DetectCodeBlocks: Analyzing {} pages...", total_pages);
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

            // Collect groups of consecutive lines that *might* be code
            let mut current_block = Vec::new();
            let mut lines_to_mark_as_code = Vec::new();

            for (idx, item) in page.items.iter().enumerate() {
                if let crate::models::ItemType::LineItem(line) = item {
                    let text = line
                        .items
                        .iter()
                        .map(|i| i.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");

                    let is_header =
                        line.height > globals.most_used_height + 1.0 || text.contains("Preface");

                    // Heuristic for code-like symbols and keywords
                    let has_code_keywords = {
                        let lower = text.to_lowercase();
                        lower.contains("import ")
                            || lower.contains("from ")
                            || lower.contains("def ")
                            || lower.contains("class ")
                            || lower.contains("try:")
                            || lower.contains("except")
                            || lower.contains("return ")
                            || lower.contains("print(")
                            || lower.contains("if ")
                            || lower.contains("for ")
                            || lower.contains("while ")
                            || lower.contains("with ")
                    };

                    let has_code_symbols = text.contains('{')
                        || text.contains('}')
                        || text.contains(';')
                        || text.contains("=>")
                        || text.contains(" = ")
                        || text.contains(" (")
                        || text.contains(" [")
                        || text.contains("] ")
                        || text.contains("):")
                        || text.contains(" # ");

                    let is_indented = line.x > indent_threshold;
                    let is_plain = line.items.iter().all(|i| i.format.is_none());
                    let has_markdown_bold =
                        text.trim().starts_with("**") && text.trim().ends_with("**");

                    let l_lower = text.to_lowercase();
                    let has_indicators = l_has_explicit_code_indicators(&text, &l_lower);

                    // A line is "code-like" if it's indented and either looks like code
                    // or is primarily plain text (not fully bold/italic).
                    // ALSO: if it has strong explicit indicators, it might be code even if not indented.
                    let looks_like_code = !is_header
                        && !has_markdown_bold
                        && ((is_indented
                            && (has_code_keywords
                                || has_code_symbols
                                || (is_plain && !text.is_empty())))
                            || has_indicators);

                    if looks_like_code {
                        current_block.push(idx);
                    } else {
                        // If we have a block of indented/code-like lines, mark them
                        if !current_block.is_empty() {
                            // If it's just one line, it MUST have code symbols/keywords or be very specific
                            if current_block.len() == 1 {
                                let line_idx = current_block[0];
                                if let crate::models::ItemType::LineItem(l) = &page.items[line_idx]
                                {
                                    let l_text = l
                                        .items
                                        .iter()
                                        .map(|i| i.text.as_str())
                                        .collect::<Vec<_>>()
                                        .join("");
                                    let l_lower = l_text.to_lowercase();
                                    if l_has_explicit_code_indicators(&l_text, &l_lower) {
                                        lines_to_mark_as_code.push(line_idx);
                                    }
                                }
                            } else {
                                // 2+ lines indented/plain/code-like -> mark as code
                                lines_to_mark_as_code.extend(current_block.iter());
                            }
                        }
                        current_block.clear();
                    }
                }
            }

            // Process last block
            if !current_block.is_empty() {
                if current_block.len() > 1 {
                    lines_to_mark_as_code.extend(current_block.iter());
                } else {
                    let line_idx = current_block[0];
                    if let crate::models::ItemType::LineItem(l) = &page.items[line_idx] {
                        let l_text = l
                            .items
                            .iter()
                            .map(|i| i.text.as_str())
                            .collect::<Vec<_>>()
                            .join("");
                        let l_lower = l_text.to_lowercase();
                        if l_has_explicit_code_indicators(&l_text, &l_lower) {
                            lines_to_mark_as_code.push(line_idx);
                        }
                    }
                }
            }

            // Apply collected code markers
            for &idx in &lines_to_mark_as_code {
                if let crate::models::ItemType::LineItem(line) = &mut page.items[idx] {
                    line.block_type = BlockType::Code;
                }
            }

            // Italic Check for User Rule: "if multiple lines in a row are highlighted in italics -> code block"
            // This is still useful for some special blocks
            let mut italic_groups = Vec::new();
            let mut current_italic_group = Vec::new();

            for (idx, item) in page.items.iter().enumerate() {
                if let crate::models::ItemType::LineItem(line) = item {
                    if line.block_type == BlockType::Code {
                        continue;
                    }
                    let text = line
                        .items
                        .iter()
                        .map(|i| i.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    let is_likely_header =
                        line.height > globals.most_used_height + 1.0 || text.contains("Preface");

                    let is_italic = !is_likely_header
                        && (line.items.iter().all(|i| {
                            matches!(
                                i.format,
                                Some(WordFormat::Italic) | Some(WordFormat::BoldItalic)
                            )
                        }) || (text.trim().starts_with('_') && text.trim().ends_with('_'))
                            || (text.trim().starts_with('*') && text.trim().ends_with('*')));

                    if is_italic {
                        current_italic_group.push(idx);
                    } else {
                        if current_italic_group.len() > 1 {
                            italic_groups.push(current_italic_group.clone());
                        }
                        current_italic_group.clear();
                    }
                }
            }
            if current_italic_group.len() > 1 {
                italic_groups.push(current_italic_group);
            }

            for group in italic_groups {
                for idx in group {
                    if let crate::models::ItemType::LineItem(line) = &mut page.items[idx] {
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

fn l_has_explicit_code_indicators(text: &str, lower: &str) -> bool {
    lower.contains("import ")
        || lower.contains("from ")
        || lower.contains("def ")
        || lower.contains("async def ")
        || lower.contains("class ")
        || lower.contains("try:")
        || lower.contains("except")
        || lower.contains("return ")
        || lower.contains("print(")
        || lower.contains("@app.")
        || lower.contains("await ")
        || lower.contains("asyncio.")
        || lower.contains("if __name__")
        || lower.starts_with("@")
        || text.contains('{')
        || text.contains('}')
        || text.contains(';')
        || text.contains("=>")
        || text.contains(" = ")
        || text.contains("):")
}
