//! Detectors turn text into candidate [`Finding`]s.
//!
//! Every detector runs over the whole input and may produce overlapping
//! findings; [`crate::redactor`] later keeps the best non-overlapping set.
//! Detectors never rewrite text themselves.

mod http;
mod keys;
mod patterns;

use crate::dictionary::{CLIENT_PLACEHOLDER, TermKind};
use crate::finding::{Category, Finding};
use crate::jwt;
use crate::redactor::Redactor;

/// Shared state for one detection pass.
pub(crate) struct Scan<'a> {
    pub r: &'a Redactor,
    pub text: &'a str,
    pub depth: usize,
    out: Vec<Finding>,
}

impl<'a> Scan<'a> {
    /// Records a finding unless the replacement is a no-op.
    pub fn push(&mut self, start: usize, end: usize, category: Category, replacement: String) {
        let original = &self.text[start..end];
        if start < end && replacement != original {
            self.out.push(Finding {
                start,
                end,
                category,
                original: original.to_string(),
                replacement,
            });
        }
    }

    /// Records an opaque credential value: decoded if it is a JWT, otherwise
    /// masked as a secret.
    pub fn token(&mut self, start: usize, end: usize) {
        let text = self.text;
        let value = &text[start..end];
        match jwt::render(self.r, value, self.depth) {
            Some(rendered) => self.push(start, end, Category::Jwt, rendered),
            None => {
                let masked = self.r.mask_secret(value);
                self.push(start, end, Category::Secret, masked);
            }
        }
    }
}

pub(crate) fn scan(r: &Redactor, text: &str, depth: usize) -> Vec<Finding> {
    let mut scan = Scan {
        r,
        text,
        depth,
        out: Vec::new(),
    };
    patterns::detect(&mut scan);
    http::detect(&mut scan);
    keys::detect(&mut scan);
    dictionary(&mut scan);
    scan.out
}

fn dictionary(scan: &mut Scan) {
    let r = scan.r;
    let text = scan.text;
    for m in r.dict.find(text) {
        let value = &text[m.start..m.end];
        let (category, replacement) = match m.kind {
            TermKind::Client => (Category::Client, CLIENT_PLACEHOLDER.to_string()),
            TermKind::User => (Category::User, r.mask_pii(value)),
            TermKind::Host => (Category::Host, r.mask_host(value)),
            TermKind::Custom(replacement) => (Category::Custom, replacement),
        };
        scan.push(m.start, m.end, category, replacement);
    }
}
