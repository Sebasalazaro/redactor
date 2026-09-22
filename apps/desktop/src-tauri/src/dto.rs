//! Data sent to the webviews.

use redactor_core::{Category, InputFormat, Redaction, Segment};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SegmentDto {
    Plain {
        text: String,
    },
    Redacted {
        category: Category,
        original: String,
        replacement: String,
    },
}

#[derive(Debug, Serialize)]
pub struct ReviewDto {
    pub format: InputFormat,
    pub engagement: Option<String>,
    pub segments: Vec<SegmentDto>,
}

impl ReviewDto {
    pub fn new(redaction: &Redaction, engagement: Option<String>) -> Self {
        let segments = redaction
            .segments()
            .into_iter()
            .map(|segment| match segment {
                Segment::Plain(text) => SegmentDto::Plain { text: text.into() },
                Segment::Redacted(f) => SegmentDto::Redacted {
                    category: f.category,
                    original: f.original.clone(),
                    replacement: f.replacement.clone(),
                },
            })
            .collect();
        Self {
            format: redaction.format,
            engagement,
            segments,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StatusDto {
    pub config_dir: String,
    pub error: Option<String>,
}
