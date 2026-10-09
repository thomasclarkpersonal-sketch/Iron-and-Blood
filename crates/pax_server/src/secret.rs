//! Passwords (D24, M4-6): the server password, the admin password, and what a client
//! sends in `Hello`. One type for all three, so a new place that holds a password
//! can't leak it by accident: its `Debug` never prints the text, and its only
//! equality takes time that depends on the lengths alone, so a client can't find a
//! password byte by byte from how fast it is refused.

/// A password. Built from text ([`Secret::new`]); never printed, compared only in
/// constant time.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(text: impl Into<String>) -> Self {
        Secret(text.into())
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.0.as_bytes(), other.0.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

impl Eq for Secret {}

impl std::fmt::Debug for Secret {
    /// Never the text: a `Config` or a `Hello` may be logged (`RUST_LOG=debug`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_compares_by_content_and_never_prints_it() {
        let s = Secret::new("s3cret");
        assert_eq!(s, Secret::new("s3cret"));
        assert_ne!(s, Secret::new("s3cres"));
        assert_ne!(s, Secret::new("s3cret!"));
        assert_ne!(s, Secret::new(""));
        assert_eq!(format!("{s:?}"), "Secret(…)");
        let hello = Some(s);
        assert!(!format!("{hello:?}").contains("s3cret"));
    }
}
