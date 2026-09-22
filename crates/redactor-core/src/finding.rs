//! What the engine found and what it replaced it with.

use serde::Serialize;

/// Kind of sensitive data. The declaration order is the priority used when
/// two findings overlap: the first one wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    PrivateKey,
    Jwt,
    Credential,
    Secret,
    Card,
    Email,
    Uuid,
    Ip,
    Host,
    Id,
    Pii,
    Client,
    User,
    Custom,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrivateKey => "private_key",
            Self::Jwt => "jwt",
            Self::Credential => "credential",
            Self::Secret => "secret",
            Self::Card => "card",
            Self::Email => "email",
            Self::Uuid => "uuid",
            Self::Ip => "ip",
            Self::Host => "host",
            Self::Id => "id",
            Self::Pii => "pii",
            Self::Client => "client",
            Self::User => "user",
            Self::Custom => "custom",
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A span of the input that was rewritten.
///
/// `start` and `end` are byte offsets into the normalized input (for Burp
/// exports, the text after base64 bodies were decoded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub start: usize,
    pub end: usize,
    pub category: Category,
    /// The original value. Never serialized, so reports cannot leak it.
    #[serde(skip)]
    pub original: String,
    pub replacement: String,
}

impl Finding {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}
