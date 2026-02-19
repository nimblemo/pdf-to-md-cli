use crate::models::{ItemType, ParseResult};
use crate::transformations::common::Transformation;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub struct RemoveRepetitiveElements {
    pub verbose: bool,
}

impl Transformation for RemoveRepetitiveElements {
    fn transform(&self, result: &mut ParseResult) {
        let total_pages = result.pages.len();
        if total_pages < 3 {
            return;
        }

        // Store min/max hash for each page
        let mut min_line_hashes: Vec<u64> = Vec::with_capacity(total_pages);
        let mut max_line_hashes: Vec<u64> = Vec::with_capacity(total_pages);

        // First pass: Calculate hashes
        for page in &result.pages {
            let (min_hash, max_hash) = calculate_page_hashes(&page.items);
            min_line_hashes.push(min_hash);
            max_line_hashes.push(max_hash);
        }

        // Count frequencies
        let mut min_freq: HashMap<u64, usize> = HashMap::new();
        let mut max_freq: HashMap<u64, usize> = HashMap::new();

        for hash in &min_line_hashes {
            if *hash != 0 {
                *min_freq.entry(*hash).or_insert(0) += 1;
            }
        }
        for hash in &max_line_hashes {
            if *hash != 0 {
                *max_freq.entry(*hash).or_insert(0) += 1;
            }
        }

        // Threshold: 2/3 of pages, minimum 3
        let threshold = (total_pages as f64 * 2.0 / 3.0).ceil() as usize;
        let threshold = threshold.max(3);

        if self.verbose {
            crate::lgger!(
                "RemoveRepetitiveElements: Analyzing {} pages...",
                result.pages.len()
            );
        }

        // Second pass: Remove items
        let mut removed_headers = 0;
        let mut removed_footers = 0;

        for (page_idx, page) in result.pages.iter_mut().enumerate() {
            let min_hash = min_line_hashes[page_idx];
            let max_hash = max_line_hashes[page_idx];

            let remove_min = min_freq.get(&min_hash).copied().unwrap_or(0) >= threshold;
            let remove_max = max_freq.get(&max_hash).copied().unwrap_or(0) >= threshold;

            if remove_min || remove_max {
                // Find min/max Y for THIS page (re-calculate as we need exact Y)
                let mut min_y = f64::MAX;
                let mut max_y = f64::MIN;

                for item in &page.items {
                    if let Some(y) = get_item_y(item) {
                        if y < min_y {
                            min_y = y;
                        }
                        if y > max_y {
                            max_y = y;
                        }
                    }
                }

                // Filter items
                let mut new_items = Vec::new();
                for item in page.items.drain(..) {
                    let mut keep = true;
                    if let Some(y) = get_item_y(&item) {
                        // Tolerance for float comparison
                        let is_min = (y - min_y).abs() < 0.001;
                        let is_max = (y - max_y).abs() < 0.001;

                        if is_min && remove_min {
                            keep = false;
                            removed_footers += 1;
                        }
                        if is_max && remove_max {
                            keep = false;
                            removed_headers += 1;
                        }
                    }
                    if keep {
                        new_items.push(item);
                    }
                }
                page.items = new_items;
            }
        }

        if self.verbose {
            crate::lgger!(
                "RemoveRepetitiveElements: Removed {} items (min Y - footer/header)",
                removed_footers
            );
            crate::lgger!(
                "RemoveRepetitiveElements: Removed {} items (max Y - header/footer)",
                removed_headers
            );
        }
    }
}

fn get_item_y(item: &ItemType) -> Option<f64> {
    match item {
        ItemType::TextItem(t) => Some(t.y),
        ItemType::LineItem(l) => Some(l.y),
        _ => None,
    }
}

fn get_item_text(item: &ItemType) -> String {
    match item {
        ItemType::TextItem(t) => t.text.clone(),
        ItemType::LineItem(l) => l.items.iter().map(|t| t.text.as_str()).collect::<String>(),
        _ => String::new(),
    }
}

fn calculate_page_hashes(items: &[ItemType]) -> (u64, u64) {
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;

    // Find ranges
    for item in items {
        if let Some(y) = get_item_y(item) {
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
        }
    }

    if min_y == f64::MAX {
        return (0, 0);
    }

    let mut min_text = String::new();
    let mut max_text = String::new();

    // Collect text
    for item in items {
        if let Some(y) = get_item_y(item) {
            if (y - min_y).abs() < 0.001 {
                min_text.push_str(&get_item_text(item));
            }
            if (y - max_y).abs() < 0.001 {
                max_text.push_str(&get_item_text(item));
            }
        }
    }

    (hash_string(&min_text), hash_string(&max_text))
}

fn hash_string(s: &str) -> u64 {
    if s.trim().is_empty() {
        return 0;
    }

    let mut hasher = DefaultHasher::new();
    // Normalize: remove digits, spaces, lowercase
    let normalized: String = s
        .chars()
        .filter(|c| !c.is_digit(10) && !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect();

    normalized.hash(&mut hasher);
    hasher.finish()
}
