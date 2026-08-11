//! Deny-by-default credential redaction for captured tool action strings
//! (ported from the `capture-trajectory.py` redactor, review tick
//! v2-001).
//!
//!
//! Everything that looks like a credential value is replaced with
//! `[REDACTED]` — known keys, header values, `-p`/`sshpass -p` arguments,
//! private-key blocks, and any UNSEEN key whose name contains a
//! credential-ish word. Zero dependencies: scanning only, no regex.
//!
//! The original key/flag text is preserved so the redacted command stays
//! readable (`password=[REDACTED]`, not a blank).

/// Marker replacing every redacted value.
pub const REDACTED: &str = "[REDACTED]";

/// Keys whose `=value` / `: value` pair is always redacted (deny-by-default).
const CREDENTIAL_KEYS: [&str; 14] = [
    "password",
    "passphrase",
    "passwd",
    "secret",
    "api_key",
    "apikey",
    "token",
    "auth",
    "cookie",
    "cred",
    "pwd",
    "session",
    "bearer",
    "sig",
];

/// True when a credential-ish word appears as a delimited component of the
/// key token (`cred-stuff`, `auth-token`) — never swallowed mid-word
/// (`mycredstuff`, `tokens_total` stay untouched).
fn is_credential_key(token: &str) -> bool {
    CREDENTIAL_KEYS.iter().any(|k| token_contains_key(token, k))
}

/// Bounded substring match: `key` must be delimited by non-alphanumerics
/// inside `token` (or the token edges).
fn token_contains_key(token: &str, key: &str) -> bool {
    if token.len() < key.len() {
        return false;
    }
    (0..=token.len() - key.len()).any(|pos| {
        token[pos..].starts_with(key)
            && token[..pos]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric())
            && token[pos + key.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric())
    })
}

/// Redact credential values in `text` (CLI command strings and JSON
/// payloads alike — both are plain text here).
#[must_use]
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    out = redact_pem_blocks(&out);
    out = redact_header_values(&out, "Cookie:");
    out = redact_header_values(&out, "Authorization:");
    out = redact_header_values(&out, "\"Cookie\":");
    out = redact_header_values(&out, "\"Authorization\":");
    out = redact_flag_values(&out);
    out = redact_key_value_pairs(&out);
    out
}

/// Replace the body of a PEM private-key block with the marker.
fn redact_pem_blocks(text: &str) -> String {
    const BEGIN: &str = "-----BEGIN ";
    const END_MARK: &str = "-----END ";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(BEGIN) {
        // The BEGIN line names the key type (`RSA PRIVATE KEY-----` /
        // `OPENSSH PRIVATE KEY-----` etc.). Require a PRIVATE KEY label.
        let after_begin = &rest[start + BEGIN.len()..];
        let Some(until) = after_begin.find("PRIVATE KEY-----") else {
            out.push_str(&rest[..start + BEGIN.len()]);
            out.push_str(after_begin);
            break;
        };
        let header_end = start + BEGIN.len() + until + "PRIVATE KEY-----".len();
        let Some(rel_end) = rest[header_end..].find(END_MARK) else {
            break;
        };
        let block_end = header_end + rel_end;
        // Include the END marker line so no key material escapes.
        out.push_str(&rest[..start]);
        out.push_str(BEGIN);
        out.push_str(&rest[start + BEGIN.len()..header_end]);
        out.push('\n');
        out.push_str(REDACTED);
        out.push('\n');
        rest = &rest[block_end..];
        if let Some(nl) = rest.find('\n') {
            out.push_str(&rest[..=nl]);
            rest = &rest[nl + 1..];
        } else {
            out.push_str(rest);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

/// Redact a `Key:` header value (plain `Cookie: x` / `Authorization:
/// Bearer y` and the JSON `"Cookie": "x"` form).
fn redact_header_values(text: &str, key: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(pos) = lower.find(key.to_ascii_lowercase().as_str()) else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..pos + key.len()]);
        rest = &rest[pos + key.len()..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with('"') {
            // JSON form: opening quote stays visible, value ends at the
            // closing quote.
            let value_start = rest.len() - trimmed.len();
            let inside = &rest[value_start + 1..];
            if let Some(end) = inside.find('"') {
                out.push_str(&rest[..=value_start]);
                out.push_str(REDACTED);
                rest = &rest[value_start + 1 + end + 1..];
                continue;
            }
        }
        // Plain form: value ends at the arg's closing quote or a newline
        // (keep the rest of the command line).
        let skip = rest.len() - rest.trim_start().len();
        let end = value_end(rest);
        if end == skip {
            // Nothing after the header (e.g. `Cookie:` alone) — keep.
            out.push_str(rest);
            break;
        }
        out.push_str(&rest[..skip]);
        out.push_str(REDACTED);
        rest = &rest[skip + end..];
    }
    out
}

/// End of a header value: at the next closing quote (`"`/`'`) or newline.
/// Deliberately ignores spaces — a `Bearer t0ken` value must redact as a
/// whole, and an unquoted multi-arg line over-redacts safely.
fn value_end(rest: &str) -> usize {
    rest.char_indices()
        .find(|&(_, c)| c == '"' || c == '\'' || c == '\n')
        .map_or(rest.len(), |(q, _)| {
            q.saturating_sub(rest.len() - rest.trim_start().len())
        })
}

/// Redact the value of `-p <value>` / `sshpass -p <value>` flags.
fn redact_flag_values(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("-p") {
        let after = pos + 2;
        let left_ok = pos == 0
            || rest[..pos]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace() || c == '=' || c == '"');
        let right_ok = rest.as_bytes().get(after).is_none_or(|b| {
            let c = char::from(*b);
            c.is_whitespace() || c == '=' || c == ':' || c == '"'
        });
        if !left_ok || !right_ok {
            // Not the `-p` flag token (e.g. the middle of `--password`).
            out.push_str(&rest[..after]);
            rest = &rest[after..];
            continue;
        }
        out.push_str(&rest[..after]);
        rest = &rest[after..];
        let trimmed = rest.trim_start();
        let skip = rest.len() - trimmed.len();
        // Do not eat a following flag as the value.
        if trimmed.starts_with('-') || trimmed.is_empty() {
            out.push_str(&rest[..skip]);
            rest = &rest[skip..];
            continue;
        }
        out.push_str(&rest[..skip]);
        // `-p=mysecret` / `--password=...`: the separator stays visible.
        let (value, eq_extra) = trimmed.strip_prefix('=').map_or((trimmed, 0), |eq| {
            out.push('=');
            let eq_trimmed = eq.trim_start();
            (eq_trimmed, 1 + eq.len() - eq_trimmed.len())
        });
        let take = value
            .find(|c: char| c.is_whitespace() || c == ',' || c == '&' || c == ';' || c == '"')
            .unwrap_or(value.len());
        out.push_str(REDACTED);
        rest = &rest[skip + eq_extra + take..];
    }
    out.push_str(rest);
    out
}

/// Redact `key=value` and `"key": "value"` pairs whose key is
/// credential-ish (deny-by-default for unseen keys).
fn redact_key_value_pairs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = find_credential_key(rest) {
        if rest[pos..].is_empty() {
            break;
        }
        let Some((key_len, sep_after, is_colon)) = find_value_separator(&rest[pos..]) else {
            let token_len = token_len_at(&rest[pos..]);
            if token_len == 0 {
                out.push_str(rest);
                break;
            }
            out.push_str(&rest[..pos + token_len]);
            rest = &rest[pos + token_len..];
            continue;
        };
        let key_text = &rest[pos..pos + key_len];
        let Some((skip, take)) = split_pair_value(&rest[pos + sep_after..], is_colon) else {
            out.push_str(&rest[..pos + sep_after]);
            rest = &rest[pos + sep_after..];
            continue;
        };
        let value = &rest[pos + sep_after + skip..pos + sep_after + skip + take];
        let quoted = value.starts_with('"') && value.ends_with('"');
        if value.trim().starts_with(REDACTED) {
            // Already redacted (e.g. a header value handled above) — keep.
            out.push_str(&rest[..pos + sep_after + skip + take]);
            rest = &rest[pos + sep_after + skip + take..];
            continue;
        }
        out.push_str(&rest[..pos]);
        out.push_str(key_text);
        out.push_str(&rest[pos + key_len..pos + sep_after]);
        if quoted {
            out.push('"');
        }
        out.push_str(REDACTED);
        if quoted {
            out.push('"');
        }
        rest = &rest[pos + sep_after + skip + take..];
    }
    out.push_str(rest);
    out
}

/// Length of the alphanumeric/`_`/`-` key token at the start of `text`.
fn token_len_at(text: &str) -> usize {
    text.char_indices()
        .take_while(|&(_, c)| c.is_alphanumeric() || c == '_' || c == '-')
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0)
}

/// Find the next credential-ish key start (case-insensitive, word-bounded).
fn find_credential_key(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    for (start, _) in lower.char_indices() {
        if !starts_word_here(text, start) {
            continue;
        }
        let token_len = token_len_at(&lower[start..]);
        if token_len == 0 {
            continue;
        }
        let token = &lower[start..start + token_len];
        if is_credential_key(token) {
            return Some(start);
        }
    }
    None
}

fn starts_word_here(text: &str, pos: usize) -> bool {
    text[..pos]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-')
}

/// Given `after_key` starting at the key token, return `(token_len,
/// sep_after, colon_sep)`: the bare key-token length and the index just
/// past the `=`/`:` separator (skipping surrounding whitespace) where the
/// value begins. Supports both `key=value` and JSON `"key": "value"`.
fn find_value_separator(after_key: &str) -> Option<(usize, usize, bool)> {
    let key_len = token_len_at(after_key);
    if key_len == 0 {
        return None;
    }
    let mut i = key_len;
    // JSON quoted key: `"key": `.
    if after_key.as_bytes().get(i) == Some(&b'"') {
        i += 1;
    }
    while after_key[i..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
    {
        i += after_key[i..].chars().next().unwrap().len_utf8();
    }
    let sep = after_key[i..].chars().next()?;
    let colon = sep == ':';
    if sep != '=' && sep != ':' {
        // Flag-style keys (`--password hunter2`) take their value as
        // the next whitespace-separated argument — the separator is
        // implicit (real defect found by falsifier: space-separated
        // flag values leaked).
        if after_key.starts_with('-') && i > key_len {
            return Some((key_len, i, false));
        }
        return None;
    }
    i += sep.len_utf8();
    while after_key[i..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
    {
        i += after_key[i..].chars().next().unwrap().len_utf8();
    }
    Some((key_len, i, colon))
}

/// Given `text` starting right after the separator, return `(skip,
/// take)` describing the value to redact: `skip` leading whitespace,
/// `take` chars of the value itself (both quotes of a JSON string
/// included). `colon_val` marks `:` records (branch secrets) — those
/// redact to the end of the line; `=` records redact one field.
fn split_pair_value(text: &str, colon_val: bool) -> Option<(usize, usize)> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let skip = text.len() - trimmed.len();
    if let Some(inner) = trimmed.strip_prefix('"') {
        let end = inner.find('"')?;
        return Some((skip, end + 2));
    }
    let take = if colon_val {
        trimmed.find('\n').unwrap_or(trimmed.len())
    } else {
        // A following flag is an argument, not a value — never eat it.
        if trimmed.starts_with('-') {
            return None;
        }
        trimmed
            .find(|c: char| {
                c.is_whitespace() || c == ',' || c == '&' || c == ';' || c == '"' || c == '\''
            })
            .unwrap_or(trimmed.len())
    };
    Some((skip, take))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(text: &str) -> String {
        redact(text)
    }

    #[test]
    fn does_not_touch_plain_text() {
        let plain = "cargo build --release && make verify";
        assert_eq!(case(plain), plain);
    }

    #[test]
    fn redacts_sshpass_and_dash_p_flag_values() {
        assert_eq!(
            case("sshpass -p hunter2 ssh host"),
            "sshpass -p [REDACTED] ssh host"
        );
        assert_eq!(
            case("curl -u user -p secret123 url"),
            "curl -u user -p [REDACTED] url"
        );
        assert_eq!(case("tool -p=mysecret run"), "tool -p=[REDACTED] run");
        // A following flag is not eaten as the value.
        assert_eq!(case("tool -p -v run"), "tool -p -v run");
    }

    #[test]
    fn redacts_known_key_equals_value_pairs() {
        assert_eq!(
            case("url?password=abc123&user=bob"),
            "url?password=[REDACTED]&user=bob"
        );
        assert_eq!(case("curl -d api_key=k123"), "curl -d api_key=[REDACTED]");
        assert_eq!(case("export SECRET=shh"), "export SECRET=[REDACTED]");
        assert_eq!(case("token = abc"), "token = [REDACTED]");
    }

    #[test]
    fn redacts_json_payload_values() {
        let json = r#"{"name": "bob", "api_key": "k123", "password": "pw", "tokens_total": 3}"#;
        let out = case(json);
        assert_eq!(
            out,
            r#"{"name": "bob", "api_key": "[REDACTED]", "password": "[REDACTED]", "tokens_total": 3}"#
        );
        assert!(!out.contains("k123") && !out.contains("pw"));
    }

    #[test]
    fn redacts_cookie_and_authorization_header_values() {
        assert_eq!(
            case("curl -H \"Cookie: session=abc123\" -H 'Authorization: Bearer t0ken'"),
            "curl -H \"Cookie: [REDACTED]\" -H 'Authorization: [REDACTED]'"
        );
        let json = r#"{"Cookie": "x=1", "Authorization": "Bearer y"}"#;
        let out = case(json);
        assert!(!out.contains("x=1") && !out.contains("Bearer y"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_pem_private_key_blocks() {
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAAB\n-----END OPENSSH PRIVATE KEY-----\n";
        let out = case(pem);
        assert!(out.contains(
            "-----BEGIN OPENSSH PRIVATE KEY-----\n[REDACTED]\n-----END OPENSSH PRIVATE KEY-----"
        ));
        assert!(!out.contains("b3BlbnNzaC1rZXktdjEAAAAAB"));
    }

    #[test]
    fn redacts_unseen_credential_ish_keys() {
        // Deny-by-default: any key carrying a credential-ish word redacts,
        // even when the compound key itself is not listed.
        assert_eq!(case("--cred-stuff=hunter2"), "--cred-stuff=[REDACTED]");
        assert_eq!(case("passphrase: correct horse"), "passphrase: [REDACTED]");
        assert_eq!(case("--auth-token=abc"), "--auth-token=[REDACTED]");
        // A prefix that merely resembles a key stays untouched.
        assert_eq!(case("--mycredstuff=hunter2"), "--mycredstuff=hunter2");
    }

    #[test]
    fn redacts_space_separated_long_flag_values() {
        // Real defect: `--password hunter2` (space form) leaked — only
        // `-p` and `key=value` were covered.
        let out = case("curl --password hunter2 https://x");
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.contains("--password [REDACTED]"), "{out}");
        assert_eq!(
            case("--secret abc --token xyz"),
            "--secret [REDACTED] --token [REDACTED]"
        );
        // A following flag is an argument, not a value: never eaten.
        assert_eq!(
            case("--password --dry-run"),
            "--password --dry-run",
            "the next flag must not be redacted as a value"
        );
        // The equals form still redacts.
        assert_eq!(case("--password=hunter2"), "--password=[REDACTED]");
    }
}
