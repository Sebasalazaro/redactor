//! Data sent to the webviews.

use redactor_core::{Category, Config, InputFormat, Redaction, Segment};
use serde::Serialize;

use crate::memory::MemoryDto;
use crate::state::LastRedaction;

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
                Segment::Redacted { finding, original } => SegmentDto::Redacted {
                    category: finding.category,
                    original: original.into(),
                    replacement: finding.replacement.clone(),
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

#[derive(Debug, Serialize)]
pub struct LastDto {
    pub output: String,
    pub format: InputFormat,
    pub categories: Vec<(Category, usize)>,
    pub engagement: Option<String>,
    pub age_secs: u64,
}

impl From<LastRedaction> for LastDto {
    fn from(last: LastRedaction) -> Self {
        Self {
            age_secs: last.at.elapsed().as_secs(),
            output: last.output,
            format: last.format,
            categories: last.categories,
            engagement: last.engagement,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OverviewDto {
    pub last: Option<LastDto>,
    pub memory: MemoryDto,
    pub redactions: u64,
    pub uptime_secs: u64,
}

/// An engagement as listed in the dashboard and the menu bar.
#[derive(Debug, Serialize)]
pub struct EngagementDto {
    pub id: String,
    pub name: String,
    pub config: Config,
}

#[derive(Debug, Serialize)]
pub struct FreedDto {
    pub released_bytes: usize,
    pub memory: MemoryDto,
}
