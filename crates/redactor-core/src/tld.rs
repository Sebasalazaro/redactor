//! Top level domains the host detector accepts.
//!
//! This is deliberately not the full IANA list. TLDs that collide with file
//! extensions or code (`.py`, `.sh`, `.md`, `.zip`, `.id`, `.at`, `.do`, ...)
//! are left out so `app.py` or `user.id` are not mistaken for hosts.

pub(crate) fn is_known(tld: &str) -> bool {
    TLDS.binary_search(&tld).is_ok()
}

/// Public suffixes made of two labels. Only affects which label is treated
/// as the registrable name.
pub(crate) fn is_second_level_suffix(suffix: &str) -> bool {
    SECOND_LEVEL.contains(&suffix)
}

// Sorted for binary search; `tests::sorted` guards it.
const TLDS: &[&str] = &[
    "agency",
    "ai",
    "app",
    "ar",
    "au",
    "bank",
    "be",
    "biz",
    "bo",
    "br",
    "ca",
    "ch",
    "cl",
    "cloud",
    "cn",
    "co",
    "com",
    "company",
    "corp",
    "cr",
    "cu",
    "de",
    "dev",
    "digital",
    "dk",
    "ec",
    "edu",
    "es",
    "eu",
    "example",
    "fi",
    "finance",
    "fr",
    "global",
    "gov",
    "group",
    "gt",
    "health",
    "hk",
    "hn",
    "home",
    "ie",
    "il",
    "in",
    "info",
    "int",
    "internal",
    "intranet",
    "invalid",
    "io",
    "it",
    "jp",
    "kr",
    "lan",
    "lat",
    "live",
    "local",
    "localdomain",
    "mil",
    "mx",
    "net",
    "network",
    "ni",
    "nl",
    "no",
    "nz",
    "online",
    "org",
    "pa",
    "pe",
    "pr",
    "pro",
    "pt",
    "ru",
    "se",
    "security",
    "services",
    "sg",
    "shop",
    "site",
    "solutions",
    "store",
    "sv",
    "systems",
    "tech",
    "test",
    "tv",
    "tw",
    "uk",
    "us",
    "uy",
    "ve",
    "xyz",
    "za",
];

const SECOND_LEVEL: &[&str] = &[
    "ac.uk", "co.in", "co.jp", "co.kr", "co.nz", "co.uk", "co.za", "com.ar", "com.au", "com.bo",
    "com.br", "com.cn", "com.co", "com.ec", "com.es", "com.mx", "com.pe", "com.sg", "com.uy",
    "com.ve", "edu.co", "gob.ar", "gob.cl", "gob.mx", "gob.pe", "gov.co", "gov.uk", "net.co",
    "org.co", "org.uk",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted() {
        assert!(TLDS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn rejects_file_extensions() {
        for ext in [
            "js", "py", "sh", "md", "json", "html", "zip", "id", "at", "do",
        ] {
            assert!(!is_known(ext), "{ext}");
        }
        assert!(is_known("com") && is_known("internal"));
    }
}
