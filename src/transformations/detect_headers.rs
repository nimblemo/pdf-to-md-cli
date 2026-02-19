use crate::models::{BlockType, ParseResult};
use crate::transformations::common::Transformation;

pub struct DetectHeaders {
    pub verbose: bool,
}

impl Transformation for DetectHeaders {
    fn transform(&self, result: &mut ParseResult) {
        let globals = &result.globals;
        let most_used_height = globals.most_used_height;
        let max_height = globals.max_height;
        let most_used_font = &globals.most_used_font;

        let mut detected_headers = 0;

        // 1. Title Page Logic
        let mut title_page_indices = std::collections::HashSet::new();
        for page in &result.pages {
            // If page has max_height text, mark as title page
            for item in &page.items {
                if let crate::models::ItemType::LineItem(line) = item {
                    let h = line.items.iter().map(|i| i.font_size).fold(0.0, f64::max);
                    if (h - max_height).abs() < 0.5 {
                        // Increased tolerance
                        title_page_indices.insert(page.index);
                        break;
                    }
                }
            }
        }

        // 2. Collect Distinct Heights (Global)
        // Only consider heights significantly larger than body text
        let threshold_ratio = 1.01;
        let min_header_height = most_used_height * threshold_ratio;

        let mut distinct_heights: Vec<f64> = Vec::new();
        for page in &result.pages {
            for item in &page.items {
                if let crate::models::ItemType::LineItem(line) = item {
                    // Check if it's a list item - skip if so
                    let text = line
                        .items
                        .iter()
                        .map(|i| &i.text)
                        .fold(String::new(), |a, b| a + b);
                    // Simple list item check (start with - or * or number.)
                    let is_list_item = text.trim().starts_with('-')
                        || text.trim().starts_with('*')
                        || (text.trim().chars().next().map_or(false, |c| c.is_numeric())
                            && text.trim().contains('.'));

                    let h = line.items.iter().map(|i| i.font_size).fold(0.0, f64::max);

                    if self.verbose && page.index == 0 {
                        crate::logger!(
                            "Page 0 Line: '{}', height={}, min_header={}",
                            text.trim(),
                            h,
                            min_header_height
                        );
                    }

                    if h > min_header_height && !is_list_item {
                        if !distinct_heights.iter().any(|&dh| (dh - h).abs() < 1.0) {
                            distinct_heights.push(h);
                        }
                    }
                }
            }
        }
        distinct_heights.sort_by(|a, b| b.partial_cmp(a).unwrap());

        if self.verbose {
            crate::logger!("DetectHeaders: distinct_heights={:?}", distinct_heights);
        }

        // 3. Apply Title Page & Height Logic
        let max_height = result.globals.max_height;
        let min_2nd_level = most_used_height + ((max_height - most_used_height) / 4.0);

        if self.verbose {
            crate::logger!(
                "DetectHeaders: most_used_height={}, max_height={}, min_2nd_level={}",
                most_used_height,
                max_height,
                min_2nd_level
            );
        }

        let most_used_dist = globals.most_used_distance;

        for page in result.pages.iter_mut() {
            let is_title_page = title_page_indices.contains(&page.index);

            for item in page.items.iter_mut() {
                if let crate::models::ItemType::LineItem(line) = item {
                    if line.block_type == BlockType::Paragraph {
                        let h = line.items.iter().map(|i| i.font_size).fold(0.0, f64::max);

                        if is_title_page {
                            if (h - max_height).abs() < 1.0 {
                                line.block_type = BlockType::H1;
                                detected_headers += 1;
                                continue;
                            } else if h >= min_2nd_level {
                                line.block_type = BlockType::H2;
                                detected_headers += 1;
                                continue;
                            }
                        }

                        // General distinct height matching
                        if h >= min_header_height {
                            if let Some(pos) =
                                distinct_heights.iter().position(|&dh| (dh - h).abs() < 1.0)
                            {
                                let level = pos + 2;
                                if level <= 6 {
                                    line.block_type = match level {
                                        2 => BlockType::H2,
                                        3 => BlockType::H3,
                                        4 => BlockType::H4,
                                        5 => BlockType::H5,
                                        _ => BlockType::H6,
                                    };
                                    detected_headers += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. All Caps/Small Headers Logic
        for page in result.pages.iter_mut() {
            let line_ys: Vec<f64> = page
                .items
                .iter()
                .filter_map(|item| {
                    if let crate::models::ItemType::LineItem(line) = item {
                        Some(line.y)
                    } else {
                        None
                    }
                })
                .collect();

            for item in page.items.iter_mut() {
                if let crate::models::ItemType::LineItem(line) = item {
                    if line.block_type == BlockType::Paragraph {
                        let text = line
                            .items
                            .iter()
                            .map(|i| i.text.as_str())
                            .collect::<Vec<_>>()
                            .join("");

                        // Check for **Wrapped Header**
                        let is_bold_wrapped =
                            text.trim().starts_with("**") && text.trim().ends_with("**");

                        if is_bold_wrapped && text.len() < 150 {
                            // Strip ** from text items
                            if let Some(first) = line.items.first_mut() {
                                if first.text.starts_with("**") {
                                    first.text = first.text.replacen("**", "", 1);
                                }
                            }
                            if let Some(last) = line.items.last_mut() {
                                if last.text.ends_with("**") {
                                    // Use range to remove last 2 chars
                                    let len = last.text.len();
                                    if len >= 2 {
                                        last.text.truncate(len - 2);
                                    }
                                }
                            }

                            // Re-evaluate text
                            let clean_text = line
                                .items
                                .iter()
                                .map(|i| i.text.as_str())
                                .collect::<Vec<_>>()
                                .join("");
                            if clean_text.trim().is_empty() {
                                continue; // Don't make empty header
                            }

                            let h = line.items.iter().map(|i| i.font_size).fold(0.0, f64::max);
                            if (h - max_height).abs() < 1.0 {
                                line.block_type = BlockType::H1;
                            } else {
                                line.block_type = BlockType::H2;
                            }
                            detected_headers += 1;
                            continue;
                        }

                        // Check for All-Bold Lines (e.g. "Join our community on", "Discord")
                        // If a line is short, isolated, and ALL bold, treat as Header.
                        let is_all_bold = line.items.iter().all(|i| {
                            matches!(
                                i.format,
                                Some(crate::models::WordFormat::Bold)
                                    | Some(crate::models::WordFormat::BoldItalic)
                            )
                        });

                        if is_all_bold && text.len() < 100 {
                            // Check isolation
                            let y = line.y;
                            let line_pos = line_ys
                                .iter()
                                .position(|&ly| (ly - y).abs() < 0.1)
                                .unwrap_or(0);
                            let mut isolated_top = true;

                            if line_pos > 0 {
                                if (line_ys[line_pos - 1] - y).abs() < most_used_dist * 1.5 {
                                    isolated_top = false;
                                }
                            }

                            if isolated_top {
                                line.block_type = BlockType::H2; // Default to H2 for bold headers
                                detected_headers += 1;
                                continue;
                            }
                        }

                        let letter_count = text.chars().filter(|c| c.is_alphabetic()).count();

                        let is_short = text.len() < 100;
                        let is_all_caps = letter_count > 0
                            && text.chars().all(|c| !c.is_alphabetic() || c.is_uppercase());

                        let font_differs = line
                            .items
                            .first()
                            .map(|ti| ti.font != *most_used_font)
                            .unwrap_or(false);

                        let y = line.y;
                        let mut isolated = true;
                        let line_pos = line_ys
                            .iter()
                            .position(|&ly| (ly - y).abs() < 0.1)
                            .unwrap_or(0);

                        if line_pos > 0 {
                            if (line_ys[line_pos - 1] - y).abs() < most_used_dist * 1.5 {
                                isolated = false;
                            }
                        }
                        if line_pos < line_ys.len() - 1 {
                            if (y - line_ys[line_pos + 1]).abs() < most_used_dist * 1.5 {
                                isolated = false;
                            }
                        }

                        if is_all_caps && isolated && font_differs && is_short {
                            line.block_type = BlockType::H6;
                            detected_headers += 1;
                        }
                    }
                }
            }
        }

        if self.verbose {
            crate::logger!("DetectHeaders: Found {} headers", detected_headers);
        }
    }
}
