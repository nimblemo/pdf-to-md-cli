use crate::models::{BlockType, ItemType, ParseResult};
use crate::transformations::common::Transformation;

pub struct ToMarkdown {
    pub verbose: bool,
}

impl Transformation for ToMarkdown {
    fn transform(&self, result: &mut ParseResult) {
        let most_used_distance = result.globals.most_used_distance;
        let mut counter = 0;
        let total = result.pages.len();

        for (page_idx, page) in result.pages.iter_mut().enumerate() {
            if self.verbose {
                counter += 1;
                if counter % 50 == 0 || counter == total {
                    eprintln!("ToMarkdown: Processed {}/{} pages...", counter, total);
                }
            }

            if page_idx == 0 && !page.items.is_empty() {
                 eprintln!("ToMarkdown: Page 0 first item: {:?}", page.items.first());
            }

            let mut markdown = String::new();
            // Removed explicit page separator here; handled in converter.rs
            
            let mut in_code_block = false;
            let mut last_y = -1.0;
            let mut last_was_header = false;

            for item in &page.items {
                let mut is_code = false;

                if let ItemType::LineItem(line) = item {
                    if line.block_type == BlockType::Code {
                        let all_bold = line.items.iter().all(|i| {
                            matches!(
                                i.format,
                                Some(crate::models::WordFormat::Bold)
                                    | Some(crate::models::WordFormat::BoldItalic)
                            )
                        });

                        if !all_bold {
                            is_code = true;
                        }
                    }

                    // Gap detection for new block/paragraph
                    if last_y > 0.0 && !last_was_header {
                        let gap = (last_y - line.y).abs();
                        // Standard line spacing is around 1.1x-1.2x. 1.25x is a safe paragraph break.
                        if gap > most_used_distance * 1.2 {
                            markdown.push_str("\n");
                        }
                    }

                    // Ensure headers have top spacing (blank line before)
                    if last_y > 0.0 && matches!(
                        line.block_type,
                        BlockType::H1 | BlockType::H2 | BlockType::H3 | BlockType::H4 | BlockType::H5 | BlockType::H6
                    ) {
                        if !markdown.ends_with("\n\n") {
                            if markdown.ends_with('\n') {
                                markdown.push('\n');
                            } else {
                                markdown.push_str("\n\n");
                            }
                        }
                    }

                    last_y = line.y;
                }

                if is_code {
                    if !in_code_block {
                        markdown.push_str("```\n");
                        in_code_block = true;
                    }
                } else if in_code_block {
                    markdown.push_str("```\n\n");
                    in_code_block = false;
                }

                match item {
                    ItemType::LineItem(line) => {
                        // For TOC items and Code, we want to preserve whitespace/indentation.
                        // For others, we normalize.
                        let text = if matches!(line.block_type, BlockType::TocItem(_) | BlockType::Code) {
                             line.items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>().join(" ")
                        } else {
                            line.items
                                .iter()
                                .flat_map(|i| i.text.split_whitespace())
                                .collect::<Vec<_>>()
                                .join(" ")
                        };

                        let is_header = matches!(
                            line.block_type,
                            BlockType::H1
                                | BlockType::H2
                                | BlockType::H3
                                | BlockType::H4
                                | BlockType::H5
                                | BlockType::H6
                        );

                        match line.block_type {
                            BlockType::H1 => {
                                let clean = text.replace("**", "").replace("_", "");
                                markdown.push_str(&format!("# {}\n\n", clean));
                            }
                            BlockType::H2 => {
                                let clean = text.replace("**", "").replace("_", "");
                                markdown.push_str(&format!("## {}\n\n", clean));
                            }
                            BlockType::H3 => {
                                let clean = text.replace("**", "").replace("_", "");
                                markdown.push_str(&format!("### {}\n\n", clean));
                            }
                            BlockType::H4 => {
                                let clean = text.replace("**", "").replace("_", "");
                                markdown.push_str(&format!("#### {}\n\n", clean));
                            }
                            BlockType::H5 => {
                                let clean = text.replace("**", "").replace("_", "");
                                markdown.push_str(&format!("##### {}\n\n", clean));
                            }
                            BlockType::H6 => {
                                let clean = text.replace("**", "").replace("_", "");
                                markdown.push_str(&format!("###### {}\n\n", clean));
                            }
                            BlockType::ListItem => markdown.push_str(&format!("- {}\n", text)),
                            BlockType::TocItem(level) => {
                                let clean = text.replace("**", "").replace("_", "");
                                let trimmed = clean.trim();
                                // Normalize spaces (e.g. "1.  First" -> "1. First")
                                let normalized = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
                                
                                // Check if it starts with a number (e.g. "1.", "10.")
                                let starts_with_number = normalized.split_whitespace().next().map_or(false, |first_word| {
                                    first_word.chars().all(|c| c.is_digit(10) || c == '.') && first_word.contains('.')
                                });

                                if starts_with_number {
                                    // Use the number as the list marker (e.g. "1. Title")
                                    markdown.push_str(&format!("{}{}\n", "   ".repeat(level), normalized));
                                } else {
                                    // Use dash as the list marker (e.g. "- Title")
                                    markdown.push_str(&format!("{}- {}\n", "   ".repeat(level), normalized));
                                }
                            }
                            BlockType::Code => {
                                let mut clean_text = text.trim_matches(|c| c == '*' || c == '_').to_string();
                                // Also remove internal bold/italic markers if they wrap the whole line?
                                // User request: "_My special thanks..._" -> "My special thanks..."
                                // If the line starts and ends with _, remove them.
                                if clean_text.starts_with('_') && clean_text.ends_with('_') {
                                    clean_text = clean_text[1..clean_text.len()-1].to_string();
                                }
                                
                                // User request: "tabulate text to the right" inside code block
                                if is_code {
                                    markdown.push_str(&format!("\t{}\n", clean_text));
                                } else {
                                    markdown.push_str(&format!("\t{}\n", clean_text));
                                }
                            }
                            BlockType::Paragraph => markdown.push_str(&format!("{}\n", text)),
                            _ => markdown.push_str(&format!("{}\n", text)),
                        }
                        last_was_header = is_header;
                    }
                    ItemType::TextItem(text_item) => {
                        markdown.push_str(&format!("{}\n", text_item.text));
                        last_was_header = false;
                    }
                    _ => {}
                }
            }

            if in_code_block {
                markdown.push_str("```\n\n");
            }

            page.items = vec![ItemType::Markdown(markdown)];
        }
    }
}
