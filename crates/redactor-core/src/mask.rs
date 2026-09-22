//! Masking primitives: how a detected value is rewritten.
//!
//! Every function is deterministic, so the same input always produces the same
//! output. That keeps redacted values consistent across a whole session, which
//! lets an LLM still follow an identifier from one request to the next.

/// Character used to hide part of a value.
pub const MASK_CHAR: char = '*';

/// Longest prefix of a secret that is ever shown, whatever the ratio.
const MAX_SECRET_PREFIX: usize = 12;

/// Secrets this short are hidden entirely.
const MIN_SECRET_WITH_PREFIX: usize = 7;

/// Hides the trailing `ratio` of the alphanumeric characters, keeping
/// separators so the value keeps its shape.
///
/// ```
/// use redactor_core::mask::cut_tail;
/// assert_eq!(cut_tail("1042", 0.2), "104*");
/// assert_eq!(cut_tail("834512", 0.2), "8345**");
/// ```
pub fn cut_tail(value: &str, ratio: f64) -> String {
    let alnum = value.chars().filter(|c| c.is_alphanumeric()).count();
    if alnum == 0 {
        return value.to_string();
    }
    let cut = ((alnum as f64 * ratio).ceil() as usize).clamp(1, alnum);
    let keep = alnum - cut;
    let mut seen = 0;
    value
        .chars()
        .map(|c| {
            if !c.is_alphanumeric() {
                return c;
            }
            seen += 1;
            if seen > keep { MASK_CHAR } else { c }
        })
        .collect()
}

/// Keeps the leading `keep_ratio` of every word and masks the rest. At least
/// one character of each word is always hidden.
///
/// ```
/// use redactor_core::mask::keep_head;
/// assert_eq!(keep_head("Juan Pérez", 0.4), "Ju** Pé***");
/// ```
pub fn keep_head(value: &str, keep_ratio: f64) -> String {
    let mut out = String::with_capacity(value.len());
    let mut word = String::new();
    for c in value.chars() {
        if c.is_alphanumeric() {
            word.push(c);
        } else {
            out.push_str(&mask_word(&word, keep_ratio));
            word.clear();
            out.push(c);
        }
    }
    out.push_str(&mask_word(&word, keep_ratio));
    out
}

fn mask_word(word: &str, keep_ratio: f64) -> String {
    let len = word.chars().count();
    if len == 0 {
        return String::new();
    }
    let keep = ((len as f64 * keep_ratio).ceil() as usize).min(len - 1);
    word.chars()
        .enumerate()
        .map(|(i, c)| if i < keep { c } else { MASK_CHAR })
        .collect()
}

/// Shows a short prefix and the length of a secret, never the whole value.
///
/// ```
/// use redactor_core::mask::secret;
/// assert_eq!(secret("abcdefghijklmnopqrst", 0.3), "abcdef…[len=20]");
/// assert_eq!(secret("hunter2", 0.3), "hu…[len=7]");
/// ```
pub fn secret(value: &str, keep_ratio: f64) -> String {
    let len = value.chars().count();
    let keep = if len < MIN_SECRET_WITH_PREFIX {
        0
    } else {
        ((len as f64 * keep_ratio).floor() as usize).min(MAX_SECRET_PREFIX)
    };
    let prefix: String = value.chars().take(keep).collect();
    format!("{prefix}…[len={len}]")
}

/// Masks an IPv4 address, keeping enough to tell internal from external
/// ranges: two octets for private addresses, one for public ones.
///
/// ```
/// use redactor_core::mask::ipv4;
/// assert_eq!(ipv4("10.20.30.40"), "10.20.*.*");
/// assert_eq!(ipv4("203.0.113.7"), "203.*.*.*");
/// ```
pub fn ipv4(ip: &str) -> String {
    let octets: Vec<&str> = ip.split('.').collect();
    let keep = if is_private_ipv4(&octets) { 2 } else { 1 };
    octets
        .iter()
        .enumerate()
        .map(|(i, o)| if i < keep { *o } else { "*" })
        .collect::<Vec<_>>()
        .join(".")
}

fn is_private_ipv4(octets: &[&str]) -> bool {
    let n: Vec<u8> = octets.iter().filter_map(|o| o.parse().ok()).collect();
    matches!(
        n.as_slice(),
        [10, ..] | [192, 168, ..] | [169, 254, ..] | [127, ..] | [100, 64..=127, ..]
    ) || matches!(n.as_slice(), [172, b, ..] if (16..=31).contains(b))
}

/// Keeps the first and last four digits of a card number, like a receipt.
///
/// ```
/// use redactor_core::mask::card;
/// assert_eq!(card("4111 1111 1111 1111"), "4111 **** **** 1111");
/// ```
pub fn card(number: &str) -> String {
    let digits = number.chars().filter(char::is_ascii_digit).count();
    let mut seen = 0;
    number
        .chars()
        .map(|c| {
            if !c.is_ascii_digit() {
                return c;
            }
            seen += 1;
            if seen <= 4 || seen > digits - 4 {
                c
            } else {
                MASK_CHAR
            }
        })
        .collect()
}

/// Luhn checksum, used to tell card numbers from other long numbers.
pub fn luhn_valid(number: &str) -> bool {
    let digits: Vec<u32> = number.chars().filter_map(|c| c.to_digit(10)).collect();
    if !(13..=19).contains(&digits.len()) {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                if d * 2 > 9 { d * 2 - 9 } else { d * 2 }
            } else {
                d
            }
        })
        .sum();
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_tail_keeps_shape() {
        assert_eq!(cut_tail("7", 0.2), "*");
        assert_eq!(cut_tail("usr_8f7a6b", 0.2), "usr_8f7a**");
        assert_eq!(
            cut_tail("550e8400-e29b-41d4-a716-446655440000", 0.2),
            "550e8400-e29b-41d4-a716-44665*******"
        );
    }

    #[test]
    fn keep_head_masks_every_word() {
        assert_eq!(keep_head("qa.tester01", 0.4), "q*.test****");
        assert_eq!(keep_head("a", 0.4), "*");
        assert_eq!(keep_head("", 0.4), "");
    }

    #[test]
    fn secret_never_reveals_short_values() {
        assert_eq!(secret("123456", 0.3), "…[len=6]");
        let long = "x".repeat(400);
        assert_eq!(secret(&long, 0.3), format!("{}…[len=400]", "x".repeat(12)));
    }

    #[test]
    fn card_numbers() {
        assert!(luhn_valid("4111111111111111"));
        assert!(!luhn_valid("4111111111111112"));
        assert!(!luhn_valid("1789996400"));
        assert_eq!(card("4111-1111-1111-1111"), "4111-****-****-1111");
    }

    #[test]
    fn ipv4_ranges() {
        assert_eq!(ipv4("172.16.0.9"), "172.16.*.*");
        assert_eq!(ipv4("172.40.0.9"), "172.*.*.*");
        assert_eq!(ipv4("192.168.1.1"), "192.168.*.*");
    }
}
