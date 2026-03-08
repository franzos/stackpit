/// Try all patterns against the text. Patterns are expected to be pre-lowercased
/// at load time -- we only lowercase the input here.
pub fn glob_match_any(patterns: &[String], text: &str) -> bool {
    let text_lower = text.to_lowercase();
    let text_bytes = text_lower.as_bytes();
    for pattern in patterns {
        if glob_match_impl(pattern.as_bytes(), text_bytes) {
            return true;
        }
    }
    false
}

/// Classic two-pointer glob matcher. Both pattern and text must already be
/// lowercased -- the caller handles that.
pub fn glob_match_impl(pattern: &[u8], text: &[u8]) -> bool {
    let mut pi = 0usize;
    let mut ti = 0usize;
    let mut star_pi = usize::MAX;
    let mut star_ti = 0usize;

    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, text: &str) -> bool {
        glob_match_any(&[pattern.to_lowercase()], text)
    }

    #[test]
    fn glob_exact_match() {
        assert!(matches("hello", "hello"));
        assert!(!matches("hello", "world"));
    }

    #[test]
    fn glob_star_wildcard() {
        assert!(matches("*", "anything"));
        assert!(matches("*timeout*", "Connection timeout error"));
        assert!(matches("*timeout*", "timeout"));
        assert!(matches("Error*", "Error: something"));
        assert!(!matches("Error*", "some Error"));
    }

    #[test]
    fn glob_question_mark() {
        assert!(matches("h?llo", "hello"));
        assert!(matches("h?llo", "hallo"));
        assert!(!matches("h?llo", "hllo"));
    }

    #[test]
    fn glob_case_insensitive() {
        assert!(matches("*NetworkError*", "networkerror in fetch"));
        assert!(matches("HELLO", "hello"));
        assert!(matches("hello", "HELLO"));
    }

    #[test]
    fn glob_empty_patterns() {
        assert!(matches("", ""));
        assert!(!matches("", "nonempty"));
        assert!(matches("*", ""));
    }

    #[test]
    fn glob_multiple_stars() {
        assert!(matches("*a*b*", "xaybz"));
        assert!(!matches("*a*b*", "xyz"));
    }
}
