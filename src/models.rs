use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub index: u16,
    pub items: Vec<ItemType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemType {
    TextItem(TextItem),
    LineItem(LineItem),
    Markdown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font: String,
    pub font_size: f64,
    pub format: Option<WordFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItem {
    pub items: Vec<TextItem>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub block_type: BlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockType {
    Paragraph,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    Code,
    ListItem,
    Footnote,
    TocItem(usize),
}

impl Default for BlockType {
    fn default() -> Self {
        BlockType::Paragraph
    }
}

pub struct ParseResult {
    pub pages: Vec<Page>,
    pub globals: GlobalStats,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalStats {
    pub most_used_height: f64,
    pub most_used_distance: f64,
    pub most_used_font: String,
    pub max_height: f64,
    pub font_to_format: HashMap<String, WordFormat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WordFormat {
    Bold,
    Italic,
    BoldItalic,
    Code,
}
