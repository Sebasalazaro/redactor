//! GLiNER's "whitespace" word splitter: words and single punctuation marks.

use std::sync::LazyLock;

use regex::Regex;

/// Same pattern as GLiNER's `WhitespaceTokenSplitter`.
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\w+(?:[-_]\w+)*|\S").unwrap());

/// A word and its byte range in the original text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Word<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn split(text: &str) -> Vec<Word<'_>> {
    WORD.find_iter(text)
        .map(|m| Word {
            text: m.as_str(),
            start: m.start(),
            end: m.end(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_and_punctuation() {
        let words: Vec<&str> = split("Hi, Maria-José! call +1 555")
            .iter()
            .map(|w| w.text)
            .collect();
        assert_eq!(
            words,
            ["Hi", ",", "Maria-José", "!", "call", "+", "1", "555"]
        );
    }

    #[test]
    fn keeps_byte_offsets() {
        let text = "Añil  Pagos";
        for w in split(text) {
            assert_eq!(&text[w.start..w.end], w.text);
        }
    }
}
