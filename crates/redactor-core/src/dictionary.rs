//! Literal term matching: client names, users, listed hosts and custom terms.
//!
//! Matching ignores case and accents (`Añil` matches `ANIL`, `añil` and
//! `anil`): patterns and text are folded to lowercase ASCII letters first,
//! with a map back to the original byte offsets. Aho-Corasick keeps hundreds
//! of terms to a single pass over the input. Client names are expanded into
//! the spellings they usually take in URLs, identifiers and headers.

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};

use crate::config::CustomTerm;

/// Placeholder that replaces any spelling of the client's name.
pub const CLIENT_PLACEHOLDER: &str = "[CLIENT]";

/// Placeholder for custom terms without an explicit replacement.
pub const DEFAULT_TERM_PLACEHOLDER: &str = "[REDACTED]";

/// Terms shorter than this only match on word boundaries, so a client called
/// `GBX` does not fire inside random strings. Longer terms also match inside
/// words and hostnames (`acme` in `acmemilesapp.com`).
const MIN_SUBSTRING_LEN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TermKind {
    Client,
    User,
    Host,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TermMatch {
    pub start: usize,
    pub end: usize,
    pub kind: TermKind,
}

/// Compiled set of patterns with the kind each one maps to.
struct Matcher {
    automaton: AhoCorasick,
    kinds: Vec<TermKind>,
    strict: Vec<bool>,
}

impl Matcher {
    fn build(entries: Vec<(String, TermKind)>) -> Option<Self> {
        let mut entries: Vec<_> = entries
            .into_iter()
            .map(|(p, k)| (fold(p.trim()).text, k))
            .filter(|(p, _)| !p.is_empty())
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.dedup_by(|a, b| a.0 == b.0);
        if entries.is_empty() {
            return None;
        }
        let automaton = AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostLongest)
            .build(entries.iter().map(|(p, _)| p))
            .expect("literal patterns always compile");
        Some(Self {
            automaton,
            strict: entries
                .iter()
                .map(|(p, _)| p.chars().count() < MIN_SUBSTRING_LEN)
                .collect(),
            kinds: entries.into_iter().map(|(_, k)| k).collect(),
        })
    }

    fn find(&self, text: &str) -> Vec<TermMatch> {
        let folded = fold(text);
        self.automaton
            .find_iter(&folded.text)
            .filter_map(|m| {
                let id = m.pattern().as_usize();
                let (start, end) = (folded.original(m.start()), folded.original(m.end()));
                if self.strict[id] && !on_word_boundary(text, start, end) {
                    return None;
                }
                Some(TermMatch {
                    start,
                    end,
                    kind: self.kinds[id].clone(),
                })
            })
            .collect()
    }
}

/// A lowercase, accent-free copy of a text that maps positions back to the
/// original. Folding `É` (2 bytes) to `e` (1 byte) shifts every later offset;
/// only those shift points are stored, so ASCII text needs no map at all.
struct Folded {
    text: String,
    /// `(folded offset, original - folded)` from that offset on, ascending.
    shifts: Vec<(usize, isize)>,
}

impl Folded {
    /// Original byte offset of a folded character boundary.
    fn original(&self, folded: usize) -> usize {
        let i = self.shifts.partition_point(|&(pos, _)| pos <= folded);
        let delta = if i == 0 { 0 } else { self.shifts[i - 1].1 };
        (folded as isize + delta) as usize
    }
}

fn fold(text: &str) -> Folded {
    if text.is_ascii() {
        return Folded {
            text: text.to_ascii_lowercase(),
            shifts: Vec::new(),
        };
    }
    let mut folded = String::with_capacity(text.len());
    let mut shifts = Vec::new();
    let mut delta = 0isize;
    for c in text.chars() {
        let f = fold_char(c);
        folded.push(f);
        let diff = c.len_utf8() as isize - f.len_utf8() as isize;
        if diff != 0 {
            delta += diff;
            shifts.push((folded.len(), delta));
        }
    }
    Folded {
        text: folded,
        shifts,
    }
}

/// One character in, one character out, so offsets map back exactly.
pub(crate) fn fold_char(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

fn on_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
}

/// Every configured literal term, ready to be searched.
pub(crate) struct Dictionary {
    all: Option<Matcher>,
    clients: Option<Matcher>,
}

impl Dictionary {
    pub fn new(
        clients: &[String],
        users: &[String],
        hosts: &[String],
        terms: &[CustomTerm],
    ) -> Self {
        let client_entries: Vec<_> = clients
            .iter()
            .flat_map(|c| client_variants(c))
            .map(|v| (v, TermKind::Client))
            .collect();

        let mut all = client_entries.clone();
        all.extend(users.iter().map(|u| (u.clone(), TermKind::User)));
        all.extend(hosts.iter().map(|h| (h.clone(), TermKind::Host)));
        all.extend(terms.iter().map(|t| {
            let replacement = t.replacement.as_deref().unwrap_or(DEFAULT_TERM_PLACEHOLDER);
            (t.value.clone(), TermKind::Custom(replacement.to_string()))
        }));

        Self {
            all: Matcher::build(all),
            clients: Matcher::build(client_entries),
        }
    }

    /// Finds every configured term in `text`.
    pub fn find(&self, text: &str) -> Vec<TermMatch> {
        self.all.as_ref().map(|m| m.find(text)).unwrap_or_default()
    }

    /// Byte ranges of client names in `text`.
    pub fn client_spans(&self, text: &str) -> Vec<(usize, usize)> {
        self.clients
            .as_ref()
            .map(|m| m.find(text).into_iter().map(|t| (t.start, t.end)).collect())
            .unwrap_or_default()
    }

    /// Rewrites `text` with `mask` applied to everything except client names,
    /// which become [`CLIENT_PLACEHOLDER`].
    pub fn mask_around_clients(&self, text: &str, mask: impl Fn(&str) -> String) -> String {
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0;
        for (start, end) in self.client_spans(text) {
            if start > cursor {
                out.push_str(&mask(&text[cursor..start]));
            }
            out.push_str(CLIENT_PLACEHOLDER);
            cursor = end;
        }
        if cursor < text.len() {
            out.push_str(&mask(&text[cursor..]));
        }
        out
    }

    /// Replaces client names only. Returns `None` when there were none.
    pub fn replace_clients(&self, text: &str) -> Option<String> {
        if self.client_spans(text).is_empty() {
            return None;
        }
        Some(self.mask_around_clients(text, str::to_string))
    }
}

/// Spellings a multi-word name takes in the wild: `Globex Bank` →
/// `globex bank`, `globex-bank`, `globex_bank`, `globex.bank`, `globexbank`.
/// CamelCase, upper case and accents are covered by folding.
pub(crate) fn client_variants(name: &str) -> Vec<String> {
    let name = fold(name.trim()).text;
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let mut variants = vec![name.clone()];
    if words.len() > 1 {
        for sep in [" ", "-", "_", ".", ""] {
            variants.push(words.join(sep));
        }
    }
    variants.sort();
    variants.dedup();
    variants.retain(|v| !v.is_empty());
    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> Dictionary {
        Dictionary::new(
            &["Globex Bank".into(), "GBX".into()],
            &["qa.tester01".into()],
            &["srv-db01".into()],
            &[CustomTerm {
                value: "MacBook-de-Pentester".into(),
                replacement: Some("[DEVICE]".into()),
            }],
        )
    }

    #[test]
    fn expands_client_spellings() {
        assert_eq!(
            client_variants("Globex Bank"),
            [
                "globex bank",
                "globex-bank",
                "globex.bank",
                "globex_bank",
                "globexbank"
            ]
        );
    }

    #[test]
    fn replaces_every_spelling_case_insensitively() {
        let d = dict();
        assert_eq!(
            d.replace_clients("GlobexBank app on api.globex-bank.example")
                .unwrap(),
            "[CLIENT] app on api.[CLIENT].example"
        );
        assert_eq!(d.replace_clients("nothing here"), None);
    }

    #[test]
    fn short_terms_need_word_boundaries() {
        let d = dict();
        assert_eq!(
            d.replace_clients("x-gbx-token").unwrap(),
            "x-[CLIENT]-token"
        );
        assert_eq!(d.replace_clients("aGBXz91"), None);
    }

    #[test]
    fn four_letter_names_match_inside_hostnames() {
        let d = Dictionary::new(&["Acme".into()], &[], &[], &[]);
        assert_eq!(
            d.replace_clients("api.acmemilesapp.com").unwrap(),
            "api.[CLIENT]milesapp.com"
        );
    }

    #[test]
    fn ignores_case_and_accents() {
        let d = Dictionary::new(&["Añil Pagos".into(), "BANCO ÉXITO".into()], &[], &[], &[]);
        assert_eq!(
            d.replace_clients("anilpagos-app AÑIL-PAGOS Añil pagos")
                .unwrap(),
            "[CLIENT]-app [CLIENT] [CLIENT]"
        );
        assert_eq!(
            d.replace_clients("bancoexito.com Banco Éxito banco_éxito")
                .unwrap(),
            "[CLIENT].com [CLIENT] [CLIENT]"
        );
    }

    #[test]
    fn folding_maps_offsets_back() {
        let text = "Éxito ñandú ok";
        let folded = fold(text);
        assert_eq!(folded.text, "exito nandu ok");
        for (i, _) in folded.text.char_indices() {
            let original = folded.original(i);
            assert!(text.is_char_boundary(original));
            assert_eq!(fold(&text[original..]).text, folded.text[i..]);
        }
        assert_eq!(folded.original(folded.text.len()), text.len());
        assert!(fold("plain ascii").shifts.is_empty());
    }

    #[test]
    fn finds_all_kinds() {
        let kinds: Vec<_> = dict()
            .find("qa.tester01 on srv-db01 from MacBook-de-Pentester")
            .into_iter()
            .map(|m| m.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                TermKind::User,
                TermKind::Host,
                TermKind::Custom("[DEVICE]".into())
            ]
        );
    }

    #[test]
    fn masks_around_clients() {
        let d = dict();
        assert_eq!(
            d.mask_around_clients("ops.globexbank", |s| "*".repeat(s.len())),
            "****[CLIENT]"
        );
    }
}
