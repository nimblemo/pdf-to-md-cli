use crate::models::{GlobalStats, Page, ParseResult, TextItem, WordFormat};
use crate::transformations::common::Transformation;
use std::collections::HashMap;

pub struct CalculateGlobalStats {
    pub verbose: bool,
}

impl Transformation for CalculateGlobalStats {
    fn transform(&self, result: &mut ParseResult) {
        if self.verbose {
            crate::logger!(
                "CalculateGlobalStats: Analyzing {} pages...",
                result.pages.len()
            );
        }
        result.globals = calculate_global_stats(&result.pages);
    }
}

fn calculate_global_stats(pages: &[Page]) -> GlobalStats {
    let mut height_counts: HashMap<String, usize> = HashMap::new();
    let mut font_counts: HashMap<String, usize> = HashMap::new();
    let mut max_height = 0.0;
    let mut max_height_font = String::new();

    // 1. Collect height and font statistics
    for page in pages {
        for item in &page.items {
            if let crate::models::ItemType::TextItem(text_item) = item {
                let lower_font = text_item.font.to_lowercase();
                if lower_font.contains("math") || lower_font.contains("symbol") {
                    continue;
                }

                let text = text_item.text.trim();
                let alpha_count = text.chars().filter(|c| c.is_alphabetic()).count();

                // Only count items that look like real words/text (at least 3 letters)
                if alpha_count < 3 {
                    continue;
                }

                // Weight by character count to ensure true body font wins.
                // Penalty for bold/italic to prefer Regular as the "Body" baseline.
                let mut weight = alpha_count;
                let lower_font = text_item.font.to_lowercase();
                if lower_font.contains("italic")
                    || lower_font.contains("oblique")
                    || lower_font.contains("bold")
                {
                    weight /= 10; // Significant penalty to prefer Regular
                }

                let height_key = format!("{:.2}", text_item.font_size);
                *height_counts.entry(height_key).or_insert(0) += weight;

                *font_counts.entry(text_item.font.clone()).or_insert(0) += weight;

                if text_item.font_size > max_height {
                    max_height = text_item.font_size;
                    max_height_font = text_item.font.clone();
                }
            }
        }
    }

    let most_used_height = get_most_used_key_as_f64(&height_counts).unwrap_or(0.0);
    let most_used_font = get_most_used_key(&font_counts).unwrap_or_default();

    // 2. Calculate most used distance
    let mut distance_counts: HashMap<String, usize> = HashMap::new();

    for page in pages {
        let mut last_item_of_most_used_height: Option<&TextItem> = None;

        for item in &page.items {
            if let crate::models::ItemType::TextItem(text_item) = item {
                let alpha_count = text_item.text.chars().filter(|c| c.is_alphabetic()).count();

                // Approximate float comparison
                if (text_item.font_size - most_used_height).abs() < 0.01 && alpha_count >= 3 {
                    if let Some(last) = last_item_of_most_used_height {
                        let dy = (last.y - text_item.y).abs();
                        if dy > 5.0 {
                            // Skip same-line or tiny jitter
                            let distance = last.y - text_item.y; // Positive if descending
                            if distance > 0.0 {
                                let dist_key = format!("{:.2}", distance);
                                *distance_counts.entry(dist_key).or_insert(0) += 1;
                            }
                        }
                    }
                    last_item_of_most_used_height = Some(text_item);
                } else if !text_item.text.trim().is_empty() {
                    last_item_of_most_used_height = None;
                }
            }
        }
    }

    let most_used_distance = get_most_used_key_as_f64(&distance_counts).unwrap_or(12.0); // Default 12 if none found

    // 3. Map fonts to formats
    let mut font_to_format = HashMap::new();
    for font_name in font_counts.keys() {
        let lower_name = font_name.to_lowercase();
        let is_bold = lower_name.contains("bold") || lower_name.contains("-bd");
        let is_italic = lower_name.contains("oblique")
            || lower_name.contains("italic")
            || lower_name.contains("-ital")
            || lower_name.contains("-it");
        let is_max_height_font = *font_name == max_height_font;

        let format = if *font_name == most_used_font {
            None
        } else if is_bold && is_italic {
            Some(WordFormat::BoldItalic)
        } else if is_bold || is_max_height_font {
            Some(WordFormat::Bold)
        } else if is_italic {
            Some(WordFormat::Italic)
        } else {
            None
        };

        if let Some(f) = format {
            font_to_format.insert(font_name.clone(), f);
        }
    }

    GlobalStats {
        most_used_height,
        most_used_distance,
        most_used_font,
        max_height,
        font_to_format,
    }
}

fn get_most_used_key(map: &HashMap<String, usize>) -> Option<String> {
    map.iter()
        .max_by_key(|entry| entry.1)
        .map(|(k, _)| k.clone())
}

fn get_most_used_key_as_f64(map: &HashMap<String, usize>) -> Option<f64> {
    get_most_used_key(map).and_then(|s| s.parse::<f64>().ok())
}
