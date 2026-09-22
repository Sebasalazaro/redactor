//! Loading a GLiNER model and running it.

use std::path::{Path, PathBuf};

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::{InputSequence, Tokenizer};

use crate::decode::{self, Entity, Span};
use crate::words::{self, Word};

/// Words per inference window. Long inputs are scanned in overlapping
/// windows so memory stays flat whatever the input size.
const WINDOW: usize = 256;
/// Words shared by consecutive windows, so an entity on a boundary is seen
/// whole by at least one of them.
const OVERLAP: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum NerError {
    #[error("model files not found in {0} (run scripts/fetch-model.sh)")]
    Missing(PathBuf),
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid gliner_config.json: {0}")]
    Config(String),
    #[error("unsupported model: {0}")]
    Unsupported(String),
    #[error("tokenizer: {0}")]
    Tokenizer(String),
    #[error("onnx runtime: {0}")]
    Runtime(#[from] ort::Error),
    #[error("worker: {0}")]
    Worker(String),
}

type Result<T> = std::result::Result<T, NerError>;

/// A loaded GLiNER model. Loading takes a moment; keep it while scanning a
/// batch and drop it afterwards to give the memory back.
pub struct Model {
    session: Session,
    tokenizer: Tokenizer,
    ent_token: String,
    sep_token: String,
}

impl Model {
    /// Loads `model.onnx`, `tokenizer.json` and `gliner_config.json` from `dir`.
    pub fn load(dir: &Path) -> Result<Self> {
        let files = ["model.onnx", "tokenizer.json", "gliner_config.json"].map(|f| dir.join(f));
        if files.iter().any(|f| !f.exists()) {
            return Err(NerError::Missing(dir.to_path_buf()));
        }
        let [onnx, tokenizer, config] = files;

        let text = std::fs::read_to_string(&config).map_err(|source| NerError::Io {
            path: config.clone(),
            source,
        })?;
        let config: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| NerError::Config(e.to_string()))?;
        let field = |name: &str| config.get(name).and_then(|v| v.as_str());
        if field("span_mode") != Some("token_level") {
            return Err(NerError::Unsupported(format!(
                "span_mode {:?}; only token-level GLiNER models are supported",
                field("span_mode").unwrap_or("?")
            )));
        }

        Ok(Self {
            session: Session::builder()?.commit_from_file(&onnx)?,
            tokenizer: Tokenizer::from_file(&tokenizer)
                .map_err(|e| NerError::Tokenizer(e.to_string()))?,
            ent_token: field("ent_token").unwrap_or("<<ENT>>").to_string(),
            sep_token: field("sep_token").unwrap_or("<<SEP>>").to_string(),
        })
    }

    /// Finds entities of the given `labels` in `text`, as byte ranges.
    pub fn detect(&mut self, text: &str, labels: &[&str], threshold: f32) -> Result<Vec<Entity>> {
        let words = words::split(text);
        if words.is_empty() || labels.is_empty() {
            return Ok(Vec::new());
        }

        let mut spans = Vec::new();
        let mut start = 0;
        loop {
            let end = (start + WINDOW).min(words.len());
            for mut span in self.run(&words[start..end], labels, threshold)? {
                span.start += start;
                span.end += start;
                spans.push(span);
            }
            if end == words.len() {
                break;
            }
            start = end - OVERLAP;
        }

        Ok(decode::greedy(spans)
            .into_iter()
            .map(|s| Entity {
                start: words[s.start].start,
                end: words[s.end].end,
                label: labels[s.label].to_string(),
                score: s.score,
            })
            .collect())
    }

    /// One inference over a window of words. Mirrors GLiNER's processor:
    /// the prompt `<<ENT>> label ... <<SEP>>` precedes the words, and
    /// `words_mask` marks the first sub-token of each text word (1-based).
    fn run(&mut self, words: &[Word], labels: &[&str], threshold: f32) -> Result<Vec<Span>> {
        let mut prompt: Vec<&str> = Vec::with_capacity(labels.len() * 2 + 1 + words.len());
        for label in labels {
            prompt.push(&self.ent_token);
            prompt.push(label);
        }
        prompt.push(&self.sep_token);
        let prompt_len = prompt.len();
        prompt.extend(words.iter().map(|w| w.text));

        let encoding = self
            .tokenizer
            .encode(InputSequence::from(prompt.as_slice()), true)
            .map_err(|e| NerError::Tokenizer(e.to_string()))?;

        let n = encoding.get_ids().len();
        let ids: Vec<i64> = encoding.get_ids().iter().map(|&i| i64::from(i)).collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&i| i64::from(i))
            .collect();
        let mut words_mask = vec![0i64; n];
        let mut previous = None;
        for (i, word) in encoding.get_word_ids().iter().enumerate() {
            if let Some(w) = *word {
                let w = w as usize;
                if previous != Some(w) && w >= prompt_len {
                    words_mask[i] = (w - prompt_len + 1) as i64;
                }
            }
            previous = word.map(|w| w as usize);
        }

        let shape = [1, n];
        let outputs = self.session.run(ort::inputs![
            "input_ids" => Tensor::from_array((shape, ids))?,
            "attention_mask" => Tensor::from_array((shape, mask))?,
            "words_mask" => Tensor::from_array((shape, words_mask))?,
            "text_lengths" => Tensor::from_array(([1, 1], vec![words.len() as i64]))?,
        ])?;

        // logits: [batch, words, labels, (start, end, inside)]
        let (dims, logits) = outputs["logits"].try_extract_tensor::<f32>()?;
        let expected = [1, words.len() as i64, labels.len() as i64, 3];
        if **dims != expected {
            return Err(NerError::Unsupported(format!(
                "logits shape {:?}, expected {expected:?}",
                &**dims
            )));
        }
        Ok(decode::spans(logits, words.len(), labels.len(), threshold))
    }
}
