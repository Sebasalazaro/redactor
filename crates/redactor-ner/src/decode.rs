//! Turns token-level logits into entities.
//!
//! For every word and label the model scores three things: the word starts
//! an entity, ends one, or is inside one. A span is kept when its first word
//! is a likely start, its last a likely end, and every word in between is
//! likely inside; its score is the mean "inside" probability.

/// An entity found in the input, as a byte range.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entity {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub score: f32,
}

/// A candidate span in word indices (inclusive), with its label index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    pub start: usize,
    pub end: usize,
    pub label: usize,
    pub score: f32,
}

/// Longest entity considered, in words (the model's `max_width`).
const MAX_WIDTH: usize = 12;

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Decodes logits laid out as `[words][labels][start, end, inside]`.
pub(crate) fn spans(logits: &[f32], words: usize, labels: usize, threshold: f32) -> Vec<Span> {
    debug_assert_eq!(logits.len(), words * labels * 3);
    let p = |word: usize, label: usize, kind: usize| {
        sigmoid(logits[(word * labels + label) * 3 + kind])
    };

    let mut spans = Vec::new();
    for label in 0..labels {
        for start in 0..words {
            if p(start, label, 0) < threshold {
                continue;
            }
            let mut inside_sum = 0.0;
            for end in start..words.min(start + MAX_WIDTH) {
                let inside = p(end, label, 2);
                if inside < threshold {
                    break; // every word of a span must be inside it
                }
                inside_sum += inside;
                if p(end, label, 1) >= threshold {
                    let score = inside_sum / (end - start + 1) as f32;
                    spans.push(Span {
                        start,
                        end,
                        label,
                        score,
                    });
                }
            }
        }
    }
    spans
}

/// Keeps the best non-overlapping spans ("flat" NER): highest score first.
pub(crate) fn greedy(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Span> = Vec::new();
    for span in spans {
        if kept
            .iter()
            .all(|k| span.end < k.start || span.start > k.end)
        {
            kept.push(span);
        }
    }
    kept.sort_by_key(|s| s.start);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds logits from probabilities, `[word][label][kind]`.
    fn logits(probs: &[[[f32; 3]; 1]]) -> Vec<f32> {
        let logit = |p: f32| (p / (1.0 - p)).ln();
        probs
            .iter()
            .flatten()
            .flatten()
            .map(|&p| logit(p))
            .collect()
    }

    #[test]
    fn finds_multi_word_spans() {
        // "Maria Gonzalez called": start at 0, end at 1, both inside.
        let l = logits(&[[[0.9, 0.1, 0.9]], [[0.1, 0.8, 0.7]], [[0.1, 0.1, 0.1]]]);
        let found = spans(&l, 3, 1, 0.5);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].start, found[0].end), (0, 1));
        assert!((found[0].score - 0.8).abs() < 1e-4);
    }

    #[test]
    fn a_gap_breaks_the_span() {
        let l = logits(&[[[0.9, 0.1, 0.9]], [[0.1, 0.1, 0.2]], [[0.1, 0.9, 0.9]]]);
        assert!(spans(&l, 3, 1, 0.5).is_empty());
    }

    #[test]
    fn greedy_keeps_best_non_overlapping() {
        let s = |start, end, score| Span {
            start,
            end,
            label: 0,
            score,
        };
        let kept = greedy(vec![s(0, 1, 0.6), s(1, 2, 0.9), s(4, 4, 0.5)]);
        assert_eq!(kept, [s(1, 2, 0.9), s(4, 4, 0.5)]);
    }
}
