use crate::models::{ItemType, LineItem, ParseResult, TextItem};
use crate::transformations::common::Transformation;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

pub struct CompactLines {
    pub verbose: bool,
}

impl Transformation for CompactLines {
    fn transform(&self, result: &mut ParseResult) {
        let most_used_distance = result.globals.most_used_distance;

        let counter = AtomicUsize::new(0);
        let total_pages = result.pages.len();

        let globals = &result.globals;

        result.pages.par_iter_mut().for_each(|page| {
            if self.verbose {
                let c = counter.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                if c % 50 == 0 || c == total_pages {
                    eprintln!("CompactLines: Processed {}/{} pages...", c, total_pages);
                }
            }

            let mut text_items: Vec<TextItem> = Vec::new();

            // Extract text items to group them
            for item in &page.items {
                if let ItemType::TextItem(ti) = item {
                    text_items.push(ti.clone());
                }
            }

            if text_items.is_empty() {
                return;
            }

            // Group by line
            let grouped_lines = group_items_by_line(text_items, most_used_distance);

            // Convert groups to LineItems
            let mut new_items = Vec::new();
            for line_group in grouped_lines {
                if let Some(line_item) = create_line_item(line_group, globals) {
                    new_items.push(ItemType::LineItem(line_item));
                }
            }

            page.items = new_items;
        });
    }
}

fn group_items_by_line(items: Vec<TextItem>, most_used_distance: f64) -> Vec<Vec<TextItem>> {
    // items.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(Ordering::Equal));

    let mut lines: Vec<Vec<TextItem>> = Vec::new();
    let mut current_line: Vec<TextItem> = Vec::new();

    for item in items {
        if let Some(first) = current_line.first() {
            // INCREASED TOLERANCE for descenders
            // A tolerance of 1.0 * font_size is safer to catch descenders like 'p', 'g', 'y'
            // that might be physically lower.
            // However, we must ensure we don't merge separate lines of text.
            // Typical line spacing is > 1.2 * font_size.
            // So 0.8 * font_size should be safe?
            let tolerance = if first.font_size > 0.0 {
                first.font_size * 0.8
            } else {
                most_used_distance // fallback
            };

            if (first.y - item.y).abs() > tolerance {
                sort_line_by_x(&mut current_line);
                lines.push(current_line);
                current_line = Vec::new();
            }
        }
        current_line.push(item);
    }

    if !current_line.is_empty() {
        sort_line_by_x(&mut current_line);
        lines.push(current_line);
    }

    lines
}

fn sort_line_by_x(line: &mut Vec<TextItem>) {
    line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
}

fn create_line_item(
    items: Vec<TextItem>,
    globals: &crate::models::GlobalStats,
) -> Option<LineItem> {
    if items.is_empty() {
        return None;
    }

    let mut merged_text_items: Vec<TextItem> = Vec::new();
    let mut current_item = items[0].clone();

    for item in items.into_iter().skip(1) {
        let gap = item.x - (current_item.x + current_item.width);
        let glue_threshold = 5.0;
        let space_threshold = (current_item.font_size * 2.0).max(30.0);
        let same_font = item.font == current_item.font;

        if gap <= glue_threshold && same_font {
            // Glue characters
            current_item.text.push_str(&item.text);
            current_item.width = (item.x + item.width) - current_item.x;
            current_item.height = current_item.height.max(item.height);
        } else if gap <= space_threshold && same_font {
            // Merge words with space
            // Check for punctuation to avoid unnecessary spaces
            let is_next_punctuation = item.text.chars().next().map_or(false, |c| ".,:;?!)]}".contains(c));
            let is_current_open_punctuation = current_item.text.chars().last().map_or(false, |c| "([{".contains(c));

            if !is_next_punctuation && !is_current_open_punctuation {
                current_item.text.push(' ');
            }
            current_item.text.push_str(&item.text);
            current_item.width = (item.x + item.width) - current_item.x;
            current_item.height = current_item.height.max(item.height);
        } else {
            merged_text_items.push(current_item);
            current_item = item;
        }
    }
    merged_text_items.push(current_item);

    // Apply formatting to the merged items
    for item in &mut merged_text_items {
        if let Some(format) = globals.font_to_format.get(&item.font) {
            let inner_text = item.text.trim();
            if inner_text.is_empty() {
                continue; // Don't format whitespace-only strings
            }

            // Check for leading/trailing whitespace to recreate it outside formatting
            let leading_space = if item.text.starts_with(' ') { " " } else { "" };
            let trailing_space = if item.text.ends_with(' ') { " " } else { "" };

            // We need to handle consecutive spaces if any?
            // "  Word  " -> " " + "**Word**" + " "
            // Re-construct logic:

            let formatted_inner = match format {
                crate::models::WordFormat::Bold => format!("**{}**", inner_text),
                crate::models::WordFormat::Italic => format!("_{}_", inner_text),
                crate::models::WordFormat::BoldItalic => format!("**_{}_**", inner_text),
                _ => inner_text.to_string(),
            };

            item.text = format!("{}{}{}", leading_space, formatted_inner, trailing_space);
        }
    }

    let x = merged_text_items.first().unwrap().x;
    let y = merged_text_items.first().unwrap().y;

    let last = merged_text_items.last().unwrap();
    let first = merged_text_items.first().unwrap();
    let width = (last.x + last.width) - first.x;

    let height = merged_text_items
        .iter()
        .map(|i| i.height)
        .fold(0.0, f64::max);

    Some(LineItem {
        items: merged_text_items,
        x,
        y,
        width,
        height,
        block_type: crate::models::BlockType::Paragraph,
    })
}
