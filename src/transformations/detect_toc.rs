use crate::models::{BlockType, ItemType, LineItem, ParseResult, TextItem};
use crate::transformations::common::Transformation;
use std::collections::{HashMap, HashSet};

pub struct DetectTOC {
    pub verbose: bool,
}

struct TocLink {
    line_item: LineItem,
    level: usize,
    original_line_index: usize,
}

struct LinkLeveler {
    level_by_method: Option<LevelMethod>,
    unique_fonts: Vec<String>,
}

#[derive(Clone, Copy)]
enum LevelMethod {
    XDiff,
    Font,
    Zero,
}

impl LinkLeveler {
    fn new() -> Self {
        LinkLeveler {
            level_by_method: None,
            unique_fonts: Vec::new(),
        }
    }

    fn level_page_items(&mut self, toc_links: &mut Vec<TocLink>) {
        if self.level_by_method.is_none() {
            let unique_x = self.calculate_unique_x(toc_links);
            if unique_x.len() > 1 {
                self.level_by_method = Some(LevelMethod::XDiff);
            } else {
                let unique_fonts = self.calculate_unique_fonts(toc_links);
                if unique_fonts.len() > 1 {
                    self.unique_fonts = unique_fonts;
                    self.level_by_method = Some(LevelMethod::Font);
                } else {
                    self.level_by_method = Some(LevelMethod::Zero);
                }
            }
        }

        match self.level_by_method.unwrap() {
            LevelMethod::XDiff => self.level_by_x_diff(toc_links),
            LevelMethod::Font => self.level_by_font(toc_links),
            LevelMethod::Zero => self.level_to_zero(toc_links),
        }
    }

    fn level_by_x_diff(&self, toc_links: &mut Vec<TocLink>) {
        let unique_x = self.calculate_unique_x(toc_links);
        for link in toc_links {
            // Find closest level
            let mut best_level = 0;
            let mut min_dist = f64::MAX;

            for (i, &ux) in unique_x.iter().enumerate() {
                let dist = (ux - link.line_item.x).abs();
                if dist < min_dist {
                    min_dist = dist;
                    best_level = i;
                }
            }
            link.level = best_level;
        }
    }

    fn level_by_font(&self, toc_links: &mut Vec<TocLink>) {
        for link in toc_links {
            // Find the most common font in the line item
            // Assuming line item has uniform font or taking the first one
            if let Some(first_text) = link.line_item.items.first() {
                if let Some(pos) = self.unique_fonts.iter().position(|f| f == &first_text.font) {
                    link.level = pos;
                }
            }
        }
    }

    fn level_to_zero(&self, toc_links: &mut Vec<TocLink>) {
        for link in toc_links {
            link.level = 0;
        }
    }

    fn calculate_unique_x(&self, toc_links: &[TocLink]) -> Vec<f64> {
        let mut unique_x: Vec<f64> = Vec::new();
        for link in toc_links {
            let x = link.line_item.x;
            // Use larger tolerance for grouping (e.g., 6.0) to handle PDF jitter and alignment differences (e.g. "10." vs "8.")
            if !unique_x.iter().any(|&ux| (ux - x).abs() < 6.0) {
                unique_x.push(x);
            }
        }
        unique_x.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        unique_x
    }

    fn calculate_unique_fonts(&self, toc_links: &[TocLink]) -> Vec<String> {
        let mut unique_fonts: Vec<String> = Vec::new();
        for link in toc_links {
            if let Some(first_text) = link.line_item.items.first() {
                let font = &first_text.font;
                if !unique_fonts.contains(font) {
                    unique_fonts.push(font.clone());
                }
            }
        }
        // No specific sort order for fonts mentioned in JS, likely order of appearance
        // But JS uses reduce without sort, so order of appearance.
        unique_fonts
    }
}

impl Transformation for DetectTOC {
    fn transform(&self, result: &mut ParseResult) {
        let max_pages_to_evaluate = std::cmp::min(20, result.pages.len());
        let mut link_leveler = LinkLeveler::new();
        // toc_pages is unused

        // We need to modify pages in place, but we need to iterate first to identify TOC pages.
        // Rust borrowing rules make this tricky.
        // We can collect indices of TOC pages first.

        let mut toc_page_indices = Vec::new();
        let mut toc_links_by_page: HashMap<usize, Vec<TocLink>> = HashMap::new();
        let mut unknown_lines_by_page: HashMap<usize, HashSet<usize>> = HashMap::new(); // page_idx -> set of line indices (original indices)

        for (page_idx, page) in result.pages.iter().enumerate().take(max_pages_to_evaluate) {
            let mut line_items_with_digits = 0;
            let mut page_toc_links = Vec::new();
            let mut unknown_lines = HashSet::new();

            let mut last_words_without_number: Option<Vec<TextItem>> = None;
            let mut last_line_idx: Option<usize> = None;
            let mut last_y: Option<f64> = None;

            let mut headline_item_idx: Option<usize> = None;

            let mut processed_items = 0;

            for (line_idx, item) in page.items.iter().enumerate() {
                if let ItemType::LineItem(line) = item {
                    processed_items += 1;

                    if page_idx == 6 && self.verbose {
                        crate::lgger!(
                            "DEBUG: Page 6 Line raw: {:?}",
                            line.items.iter().map(|w| &w.text).collect::<Vec<_>>()
                        );
                    }

                    // Logic to extract digits at end of line
                    // Rust equivalent of JS logic:
                    // words = line.words.filter(...)
                    // while words.last is number -> pop

                    let mut words: Vec<TextItem> = line.items.clone();
                    // Filter out words that are only dots
                    words.retain(|w| !w.text.chars().all(|c| c == '.'));

                    let mut digits = String::new();

                    // Check last word for digits, ignoring trailing markdown/dots
                    if let Some(last_idx) = words.len().checked_sub(1) {
                        let last = &mut words[last_idx];
                        let original_text = last.text.clone();

                        // 1. Remove trailing markdown chars (*, _, space) for check
                        let trimmed_text =
                            original_text.trim_end_matches(|c| c == '*' || c == '_' || c == ' ');

                        // 2. Check if it ends with digits
                        let mut suffix_digits = String::new();
                        let mut temp_text = trimmed_text.to_string();

                        while let Some(c) = temp_text.chars().last() {
                            if c.is_digit(10) {
                                suffix_digits.insert(0, c);
                                temp_text.pop();
                            } else {
                                break;
                            }
                        }

                        if !suffix_digits.is_empty() {
                            digits = suffix_digits;

                            // 3. Update the last word text
                            // Find last occurrence of digits in original_text
                            if let Some(pos) = original_text.rfind(&digits) {
                                // Everything before digits is the candidate for new text
                                let prefix = &original_text[..pos];
                                // Everything after digits is the trailing markdown
                                let suffix = &original_text[pos + digits.len()..];

                                // Clean the prefix (remove trailing dots/spaces)
                                let clean_prefix =
                                    prefix.trim_end_matches(|c| c == '.' || c == ' ');

                                // Reconstruct
                                last.text = format!("{}{}", clean_prefix, suffix);
                            }
                        }
                    }

                    // Remove empty words (e.g. if word was just digits)
                    words.retain(|w| !w.text.is_empty());

                    let ends_with_digit = !digits.is_empty();

                    if page_idx == 6 && self.verbose {
                        let text = words
                            .iter()
                            .map(|w| w.text.as_str())
                            .collect::<Vec<_>>()
                            .join(" ");
                        crate::lgger!("DEBUG: Page 6 processed words: {:?}, digits: '{}', ends_with_digit: {}", 
                             words.iter().map(|w| &w.text).collect::<Vec<_>>(),
                             digits,
                             ends_with_digit
                         );
                        if !digits.is_empty() || text.contains("First Things First") {
                            crate::lgger!("DEBUG: detailed digit check for '{}':", text);
                            if let Some(last_idx) = words.len().checked_sub(1) {
                                let last = &words[last_idx];
                                let original = &last.text;
                                let trimmed =
                                    original.trim_end_matches(|c| c == '*' || c == '_' || c == ' ');
                                crate::lgger!("  Original: '{}'", original);
                                crate::lgger!("  Trimmed: '{}'", trimmed);
                                if let Some(c) = trimmed.chars().last() {
                                    crate::lgger!(
                                        "  Last char: '{}', is_digit: {}",
                                        c,
                                        c.is_digit(10)
                                    );
                                } else {
                                    crate::lgger!("  Trimmed is empty");
                                }
                            }
                        }
                    }

                    let ends_with_digit = Self::line_ends_with_digit(line);

                    if ends_with_digit {
                        if let Some(prev_words) = last_words_without_number.take() {
                            // Check gap to avoid merging unrelated headers
                            let threshold = result.globals.most_used_distance * 1.5;
                            let gap = (last_y.unwrap_or(line.y) - line.y).abs();

                            if page_idx == 6 && self.verbose {
                                crate::lgger!(
                                    "DEBUG: Merge check. gap={:.2}, threshold={:.2}",
                                    gap,
                                    threshold
                                );
                            }

                            if gap > threshold {
                                // Don't merge. Treat previous as unknown (likely header)
                                if let Some(idx) = last_line_idx {
                                    unknown_lines.insert(idx);
                                }
                                // Current line is start of new item
                            } else {
                                // Merge
                                let mut new_words = prev_words;
                                new_words.extend(words);
                                words = new_words;
                            }
                        }

                        // Create a new LineItem for the TOC link
                        let mut link_line_item = line.clone();
                        link_line_item.items = words;

                        // Update x/y to match the first item (in case of multiline merge where first line starts earlier)
                        if let Some(first_item) = link_line_item.items.first() {
                            link_line_item.x = first_item.x;
                            link_line_item.y = first_item.y;
                        }

                        if self.verbose {
                            let text = link_line_item
                                .items
                                .iter()
                                .map(|w| w.text.as_str())
                                .collect::<String>();
                            if text.contains("Learning from Failure")
                                || text.contains("Talking About Failure")
                                || text.contains("Postincident Reviews")
                            {
                                crate::lgger!(
                                    "DEBUG: TOC Item '{}' X={:.2}",
                                    text,
                                    link_line_item.x
                                );
                            }
                            if link_line_item.x > 77.0 && link_line_item.x < 80.0 {
                                crate::lgger!(
                                    "DEBUG: Found item at X={:.2}: '{}'",
                                    link_line_item.x,
                                    text
                                );
                            }
                        }

                        page_toc_links.push(TocLink {
                            line_item: link_line_item,
                            level: 0, // Will be set by leveler
                            original_line_index: line_idx,
                        });

                        line_items_with_digits += 1;
                    } else {
                        if headline_item_idx.is_none() {
                            headline_item_idx = Some(line_idx);
                        } else {
                            if let Some(idx) = last_line_idx {
                                // Previous line was also without number, mark it unknown
                                unknown_lines.insert(idx);
                            }
                            last_words_without_number = Some(words);
                            last_line_idx = Some(line_idx);
                            last_y = Some(line.y);
                        }
                    }
                }
            }

            if processed_items > 0
                && (line_items_with_digits as f64 * 100.0 / processed_items as f64) > 75.0
            {
                toc_page_indices.push(page_idx);

                // Level items
                link_leveler.level_page_items(&mut page_toc_links);

                toc_links_by_page.insert(page_idx, page_toc_links);
                unknown_lines_by_page.insert(page_idx, unknown_lines);

                if self.verbose {
                    crate::lgger!("DEBUG: Detected TOC page {}", page.index);
                }
            }
        }

        let mut first_page_headers: HashSet<String> = HashSet::new();

        // Now apply changes to the pages
        for (seq_idx, &page_idx) in toc_page_indices.iter().enumerate() {
            if let Some(page) = result.pages.get_mut(page_idx) {
                let toc_links = toc_links_by_page.get(&page_idx).unwrap();
                let unknown_lines = unknown_lines_by_page.get(&page_idx).unwrap();

                // Filter unknown lines that are contained in TOC links (duplicates)
                let mut valid_unknown_lines = HashSet::new();
                for &idx in unknown_lines {
                    let line_text = match &page.items[idx] {
                        ItemType::LineItem(item) => item
                            .items
                            .iter()
                            .map(|i| i.text.as_str())
                            .collect::<String>(),
                        _ => String::new(),
                    };

                    let clean_line_text = line_text.trim();
                    if clean_line_text.is_empty() {
                        valid_unknown_lines.insert(idx);
                        continue;
                    }

                    // Remove repetitive headers found on the first TOC page (Dynamic with partial matching)
                    let check_repetitive = |text: &str| -> bool {
                        if first_page_headers.contains(text) {
                            return true;
                        }
                        // Check if text starts with any of the first page headers (e.g. "Table of Contents 5" starts with "Table of Contents")
                        // But ensure the header is significant (e.g. > 3 chars) to avoid matching "1" against "10"
                        if first_page_headers
                            .iter()
                            .any(|h| h.len() > 3 && text.starts_with(h))
                        {
                            return true;
                        }

                        // Check normalized version (split by |)
                        let text_norm = text.split('|').next().unwrap_or(text).trim();
                        if first_page_headers.contains(text_norm) {
                            return true;
                        }
                        if first_page_headers
                            .iter()
                            .any(|h| h.len() > 3 && text_norm.starts_with(h))
                        {
                            return true;
                        }

                        false
                    };

                    if seq_idx > 0 && check_repetitive(clean_line_text) {
                        if self.verbose {
                            crate::lgger!(
                                "DEBUG: Removing repetitive TOC header (dynamic): '{}'",
                                clean_line_text
                            );
                        }
                        continue;
                    }

                    // Remove repetitive "Table of Contents" headers on subsequent pages (Fallback for partial matches)
                    if seq_idx > 0 && clean_line_text.to_lowercase().contains("table of contents") {
                        if self.verbose {
                            crate::lgger!(
                                "DEBUG: Removing repetitive TOC header: '{}'",
                                clean_line_text
                            );
                        }
                        continue;
                    }

                    let is_duplicate = toc_links.iter().any(|link| {
                        let link_text = link
                            .line_item
                            .items
                            .iter()
                            .map(|i| i.text.as_str())
                            .collect::<String>();
                        link_text.contains(clean_line_text)
                    });

                    if !is_duplicate {
                        valid_unknown_lines.insert(idx);
                        if seq_idx == 0 {
                            if self.verbose {
                                crate::lgger!("DEBUG: Learned TOC header: '{}'", clean_line_text);
                            }
                            first_page_headers.insert(clean_line_text.to_string());

                            // Also learn normalized version
                            let norm = clean_line_text
                                .split('|')
                                .next()
                                .unwrap_or(clean_line_text)
                                .trim();
                            if norm != clean_line_text {
                                first_page_headers.insert(norm.to_string());
                            }
                        }
                    } else if self.verbose {
                        crate::lgger!(
                            "DEBUG: Removing duplicate unknown line {}: '{}'",
                            idx,
                            clean_line_text
                        );
                    }
                }

                let mut toc_link_map: HashMap<usize, &TocLink> = HashMap::new();
                for link in toc_links {
                    toc_link_map.insert(link.original_line_index, link);
                }

                let current_items = std::mem::take(&mut page.items);
                // Identify headline index again
                let mut headline_idx = None;
                for (i, item) in current_items.iter().enumerate() {
                    if let ItemType::LineItem(line) = item {
                        // Check for repetitive header
                        let text = line
                            .items
                            .iter()
                            .map(|w| w.text.as_str())
                            .collect::<String>();
                        let clean_text = text.trim();

                        // Reuse check_repetitive logic? We can't easily reuse the closure from previous scope.
                        // Duplicate logic briefly or move to method.
                        // Inline check:
                        let is_repetitive = {
                            if first_page_headers.contains(clean_text) {
                                true
                            } else if first_page_headers
                                .iter()
                                .any(|h| h.len() > 3 && clean_text.starts_with(h))
                            {
                                true
                            } else {
                                let text_norm =
                                    clean_text.split('|').next().unwrap_or(clean_text).trim();
                                if first_page_headers.contains(text_norm) {
                                    true
                                } else if first_page_headers
                                    .iter()
                                    .any(|h| h.len() > 3 && text_norm.starts_with(h))
                                {
                                    true
                                } else {
                                    false
                                }
                            }
                        };

                        if seq_idx > 0 && is_repetitive {
                            continue;
                        }

                        if seq_idx > 0 && text.to_lowercase().contains("table of contents") {
                            continue;
                        }

                        let ends_with_digit = Self::line_ends_with_digit(line);
                        if !ends_with_digit {
                            headline_idx = Some(i);
                            break;
                        }
                    }
                }

                let mut new_items = Vec::new();

                for (i, item) in current_items.into_iter().enumerate() {
                    // Check if this index corresponds to a TOC link
                    if let Some(link) = toc_link_map.get(&i) {
                        let mut item = link.line_item.clone();
                        item.block_type = BlockType::TocItem(link.level);
                        new_items.push(ItemType::LineItem(item));
                        continue;
                    }

                    if valid_unknown_lines.contains(&i) {
                        new_items.push(item);
                    } else if Some(i) == headline_idx {
                        new_items.push(item);
                    }
                    // Else discarded
                }

                page.items = new_items;
            }
        }
    }
}

impl DetectTOC {
    fn line_ends_with_digit(line: &LineItem) -> bool {
        let mut words: Vec<TextItem> = line.items.clone();
        words.retain(|w| !w.text.chars().all(|c| c == '.'));

        if let Some(last) = words.last() {
            let original_text = &last.text;
            // Remove trailing markdown
            let trimmed_text = original_text.trim_end_matches(|c| c == '*' || c == '_' || c == ' ');
            // Check if ends with digit
            if let Some(c) = trimmed_text.chars().last() {
                return c.is_digit(10);
            }
        }

        false
    }
}
