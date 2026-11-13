//! Secret redactor: pure function core (no axum, no logging of user content).
//!
//! Single-pass, deterministic: protected spans (existing placeholders) are never
//! matched again, which keeps `redact` idempotent; built-in rules never match `\n`,
//! and custom words containing `\n` keep their newlines, so line count is preserved.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::Ipv6Addr;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

pub const MAX_TEXT_BYTES: usize = 5_242_880;
pub const MAX_CUSTOM_WORDS: usize = 100;
pub const MAX_CUSTOM_WORD_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Jwt,
    Bearer,
    ApiKey,
    EnvValue,
    Email,
    Phone,
    Ipv4,
    Ipv6,
    CreditCard,
    Custom,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Jwt,
        Category::Bearer,
        Category::ApiKey,
        Category::EnvValue,
        Category::Email,
        Category::Phone,
        Category::Ipv4,
        Category::Ipv6,
        Category::CreditCard,
        Category::Custom,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Jwt => "jwt",
            Category::Bearer => "bearer",
            Category::ApiKey => "api_key",
            Category::EnvValue => "env_value",
            Category::Email => "email",
            Category::Phone => "phone",
            Category::Ipv4 => "ipv4",
            Category::Ipv6 => "ipv6",
            Category::CreditCard => "credit_card",
            Category::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.iter().copied().find(|c| c.as_str() == s)
    }

    pub fn placeholder(&self) -> String {
        format!("[REDACTED:{}]", self.as_str())
    }

    /// Lower value wins when two candidates have the same start and length.
    fn priority(&self) -> u8 {
        match self {
            Category::Custom => 0,
            Category::Jwt => 1,
            Category::Bearer => 2,
            Category::ApiKey => 3,
            Category::Email => 4,
            Category::Ipv4 => 5,
            Category::Ipv6 => 6,
            Category::CreditCard => 7,
            Category::Phone => 8,
            Category::EnvValue => 9,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Auto,
    Env,
    Text,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        match s {
            "auto" => Some(Format::Auto),
            "env" => Some(Format::Env),
            "text" => Some(Format::Text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactOptions {
    pub categories: BTreeSet<Category>,
    pub custom_words: Vec<String>,
    pub format: Format,
}

impl Default for RedactOptions {
    fn default() -> Self {
        RedactOptions {
            categories: Category::ALL.iter().copied().collect(),
            custom_words: Vec::new(),
            format: Format::Auto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactOutcome {
    pub text: String,
    pub counts: BTreeMap<Category, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactError {
    TooManyWords,
    EmptyWord,
    WordTooLong,
}

impl fmt::Display for RedactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedactError::TooManyWords => write!(
                f,
                "Daftar kata kustom melebihi batas {} kata",
                MAX_CUSTOM_WORDS
            ),
            RedactError::EmptyWord => write!(f, "Kata kustom tidak boleh kosong"),
            RedactError::WordTooLong => write!(
                f,
                "Kata kustom melebihi batas {} karakter",
                MAX_CUSTOM_WORD_CHARS
            ),
        }
    }
}

impl std::error::Error for RedactError {}

/// Validates custom words (HTTP 400 on error). Never quotes word content.
pub fn validate(opts: &RedactOptions) -> Result<(), RedactError> {
    if opts.custom_words.len() > MAX_CUSTOM_WORDS {
        return Err(RedactError::TooManyWords);
    }
    for w in &opts.custom_words {
        if w.is_empty() {
            return Err(RedactError::EmptyWord);
        }
        if w.chars().count() > MAX_CUSTOM_WORD_CHARS {
            return Err(RedactError::WordTooLong);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Regexes
// ---------------------------------------------------------------------------

struct Rules {
    protected: Regex,
    env_line: Regex,
    jwt: Regex,
    bearer: Regex,
    api_prefix: Regex,
    api_kv: Regex,
    email: Regex,
    ipv4: Regex,
    ipv6: Regex,
    card: Regex,
    phone: Regex,
}

fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| {
        let octet = r"(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])";
        let ipv4 = format!(r"{o}(?:\.{o}){{3}}", o = octet);
        Rules {
            protected: Regex::new(
                r"\[REDACTED:(?:jwt|bearer|api_key|env_value|email|phone|ipv4|ipv6|credit_card|custom)\]\n*",
            )
            .unwrap(),
            env_line: Regex::new(r"^(\s*(?:export\s+)?[A-Za-z_][A-Za-z0-9_]*=)(.*)$").unwrap(),
            jwt: Regex::new(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]*").unwrap(),
            bearer: Regex::new(r"(?i)bearer[ \t]+([A-Za-z0-9._~+/=-]{8,})").unwrap(),
            api_prefix: Regex::new(
                r"sk-[A-Za-z0-9_-]{16,}|AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{20,}|xox[abprs]-[A-Za-z0-9-]{10,}|AIza[0-9A-Za-z_-]{35}",
            )
            .unwrap(),
            api_kv: Regex::new(
                r#"(?i)[A-Za-z0-9_-]*(?:api[_-]?key|secret|token|password|passwd)[A-Za-z0-9_-]*["']?[ \t]*[:=][ \t]*["']?([^\s"',;]{4,})"#,
            )
            .unwrap(),
            email: Regex::new(
                r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}",
            )
            .unwrap(),
            ipv4: Regex::new(&ipv4).unwrap(),
            ipv6: Regex::new(r"[0-9A-Fa-f]{0,4}(?::[0-9A-Fa-f]{0,4}){2,7}").unwrap(),
            card: Regex::new(r"[0-9](?:[ -]?[0-9]){12,18}").unwrap(),
            phone: Regex::new(r"\+[0-9](?:[ .()-]{0,2}[0-9]){7,14}").unwrap(),
        }
    })
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

struct Cand {
    start: usize,
    end: usize,
    cat: Category,
    /// Reject when an adjacent char is alphanumeric and not itself replaced.
    bounded: bool,
}

fn luhn_ok(digits: &[u32]) -> bool {
    let mut sum = 0;
    for (i, d) in digits.iter().rev().enumerate() {
        let mut v = *d;
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    sum % 10 == 0
}

fn ipv6_plausible(s: &str) -> bool {
    if s.parse::<Ipv6Addr>().is_err() {
        return false;
    }
    if s.matches(':').count() >= 3 || s == "::1" {
        return true;
    }
    match s.find("::") {
        Some(i) => {
            let left = s[..i].chars().any(|c| c.is_ascii_hexdigit());
            let right = s[i + 2..].chars().any(|c| c.is_ascii_hexdigit());
            left && right
        }
        None => false,
    }
}

/// True when nothing right after `pos` continues the token (no word char, `:`,
/// or another separator-then-digit group the capped phone regex stopped before).
fn phone_clean_after(seg: &str, pos: usize) -> bool {
    let rest = &seg[pos..];
    match rest.chars().next() {
        None => true,
        Some(c) if is_word(c) || c == ':' => false,
        Some(_) => {
            let sep = rest
                .char_indices()
                .take(2)
                .take_while(|(_, c)| matches!(c, ' ' | '.' | '(' | ')' | '-'))
                .last()
                .map_or(0, |(i, c)| i + c.len_utf8());
            !rest[sep..].starts_with(|c: char| c.is_ascii_digit())
                && !rest.starts_with(|c: char| c.is_ascii_digit())
        }
    }
}

/// Length of the phone number starting at `start`, given the greedy regex match `s`.
/// The greedy match can swallow an adjacent token after a space ("+1000000000 0.0.0.0",
/// "+1000000000 75a:fa::1", "+6322346005 1000 0000 0000 9"); in that case only the
/// first whitespace-free chunk is kept.
fn phone_len(seg: &str, start: usize, s: &str) -> Option<usize> {
    let mut len = s.len();
    if let Some(ws) = s.find(char::is_whitespace) {
        let swallows_ipv4 = s[ws..].split_whitespace().any(|chunk| {
            rules()
                .ipv4
                .find(chunk)
                .map_or(false, |m| m.start() == 0 && m.end() == chunk.len())
        });
        if swallows_ipv4 || !phone_clean_after(seg, start + s.len()) {
            len = s[..ws].trim_end_matches(|c: char| !c.is_ascii_digit()).len();
        }
    }
    let digits = s[..len].chars().filter(|c| c.is_ascii_digit()).count();
    (digits >= 8).then_some(len)
}

fn gather_segment(
    seg: &str,
    base: usize,
    active: &BTreeSet<Category>,
    words: &[&str],
    out: &mut Vec<Cand>,
) {
    let r = rules();
    let mut push = |s: usize, e: usize, cat: Category, bounded: bool| {
        out.push(Cand { start: base + s, end: base + e, cat, bounded });
    };
    if active.contains(&Category::Jwt) {
        for m in r.jwt.find_iter(seg) {
            push(m.start(), m.end(), Category::Jwt, false);
        }
    }
    if active.contains(&Category::Bearer) {
        for c in r.bearer.captures_iter(seg) {
            if let Some(g) = c.get(1) {
                push(g.start(), g.end(), Category::Bearer, false);
            }
        }
    }
    if active.contains(&Category::ApiKey) {
        for m in r.api_prefix.find_iter(seg) {
            push(m.start(), m.end(), Category::ApiKey, false);
        }
        for c in r.api_kv.captures_iter(seg) {
            if let Some(g) = c.get(1) {
                push(g.start(), g.end(), Category::ApiKey, false);
            }
        }
    }
    if active.contains(&Category::Email) {
        for m in r.email.find_iter(seg) {
            push(m.start(), m.end(), Category::Email, false);
        }
    }
    if active.contains(&Category::Ipv4) {
        for m in r.ipv4.find_iter(seg) {
            push(m.start(), m.end(), Category::Ipv4, true);
        }
    }
    if active.contains(&Category::Ipv6) {
        for m in r.ipv6.find_iter(seg) {
            if ipv6_plausible(m.as_str()) {
                push(m.start(), m.end(), Category::Ipv6, false);
            }
        }
    }
    if active.contains(&Category::CreditCard) {
        let mut at = 0usize;
        while let Some(m) = r.card.find_at(seg, at) {
            // A digit run right after `+` belongs to a phone number, never a card.
            if seg[..m.start()].ends_with('+') {
                at = m.start() + 1;
                continue;
            }
            // Nor can a card start inside a dotted numeric token ("1.2.3.<card>"):
            // those digits belong to an IPv4 address or a decimal number.
            if dotted_digit_before(seg, m.start()) {
                at = m.start() + 1;
                continue;
            }
            // The regex is greedy and may swallow digits of an adjacent number
            // ("<card> 0.0.0.0"); take the longest Luhn-valid prefix ending on a digit.
            let s = m.as_str();
            let ends: Vec<usize> = s
                .char_indices()
                .filter(|(_, c)| c.is_ascii_digit())
                .map(|(i, c)| i + c.len_utf8())
                .collect();
            // Every Luhn-valid prefix is a candidate; `select` prefers the longest and
            // falls back to a shorter one when the longest is rejected by the boundary check.
            for (n, end) in ends.iter().enumerate() {
                if n + 1 < 13 {
                    continue;
                }
                let digits: Vec<u32> =
                    s[..*end].chars().filter_map(|c| c.to_digit(10)).collect();
                if luhn_ok(&digits) && !dotted_digit_after(seg, m.start() + *end) {
                    push(m.start(), m.start() + *end, Category::CreditCard, true);
                }
            }
            // Retry from the next digit: the match may have started inside a
            // preceding digit run ("A000 <card>"); overlaps are resolved in `select`.
            at = m.start() + 1;
        }
    }
    if active.contains(&Category::Phone) {
        for m in r.phone.find_iter(seg) {
            if let Some(len) = phone_len(seg, m.start(), m.as_str()) {
                push(m.start(), m.start() + len, Category::Phone, false);
            }
        }
    }
    if active.contains(&Category::Custom) {
        for w in words {
            for (i, m) in seg.match_indices(*w) {
                push(i, i + m.len(), Category::Custom, false);
            }
        }
    }
}

/// True when `seg[..pos]` ends with `<digit>.` (pos sits inside a dotted number).
fn dotted_digit_before(seg: &str, pos: usize) -> bool {
    let mut it = seg[..pos].chars().rev();
    it.next() == Some('.') && it.next().map_or(false, |c| c.is_ascii_digit())
}

/// True when `seg[pos..]` starts with `.<digit>` (pos sits inside a dotted number).
fn dotted_digit_after(seg: &str, pos: usize) -> bool {
    let mut it = seg[pos..].chars();
    it.next() == Some('.') && it.next().map_or(false, |c| c.is_ascii_digit())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Greedy non-overlapping selection, then drop bounded candidates whose
/// neighbours are word chars that are not part of another replacement.
fn select(text: &str, mut cands: Vec<Cand>) -> Vec<Cand> {
    cands.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then((b.end - b.start).cmp(&(a.end - a.start)))
            .then(a.cat.priority().cmp(&b.cat.priority()))
    });
    loop {
        let mut chosen: Vec<&Cand> = Vec::new();
        let mut last_end = 0usize;
        for c in &cands {
            if c.start >= last_end && c.end > c.start {
                chosen.push(c);
                last_end = c.end;
            }
        }
        let in_chosen = |idx: usize| -> bool {
            let p = chosen.partition_point(|c| c.start <= idx);
            p > 0 && chosen[p - 1].end > idx
        };
        let mut reject: Vec<(usize, usize)> = Vec::new();
        for c in &chosen {
            if !c.bounded {
                continue;
            }
            let left = text[..c.start].char_indices().next_back();
            let right = text[c.end..].chars().next().map(|ch| (c.end, ch));
            let bad_left = matches!(left, Some((i, ch)) if is_word(ch) && !in_chosen(i));
            let bad_right = matches!(right, Some((i, ch)) if is_word(ch) && !in_chosen(i));
            if bad_left || bad_right {
                reject.push((c.start, c.end));
            }
        }
        if reject.is_empty() {
            let keep: Vec<(usize, usize)> = chosen.iter().map(|c| (c.start, c.end)).collect();
            return cands
                .into_iter()
                .filter(|c| keep.binary_search(&(c.start, c.end)).is_ok())
                .fold(Vec::new(), |mut acc: Vec<Cand>, c| {
                    if acc.last().map_or(true, |l| c.start >= l.end) {
                        acc.push(c);
                    }
                    acc
                });
        }
        cands.retain(|c| !(c.bounded && reject.contains(&(c.start, c.end))));
    }
}

fn redact_fragment(
    text: &str,
    active: &BTreeSet<Category>,
    words: &[&str],
    counts: &mut BTreeMap<Category, usize>,
) -> String {
    if text.is_empty() {
        return String::new();
    }
    let r = rules();
    let protected: Vec<(usize, usize)> =
        r.protected.find_iter(text).map(|m| (m.start(), m.end())).collect();

    let mut cands = Vec::new();
    let mut pos = 0usize;
    for (ps, pe) in &protected {
        gather_segment(&text[pos..*ps], pos, active, words, &mut cands);
        pos = *pe;
    }
    gather_segment(&text[pos..], pos, active, words, &mut cands);

    let chosen = select(text, cands);

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for c in &chosen {
        out.push_str(&text[cursor..c.start]);
        out.push_str(&c.cat.placeholder());
        for _ in 0..text[c.start..c.end].matches('\n').count() {
            out.push('\n');
        }
        *counts.entry(c.cat).or_insert(0) += 1;
        cursor = c.end;
    }
    out.push_str(&text[cursor..]);
    out
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

fn detect_env(text: &str) -> bool {
    let r = rules();
    let mut any = false;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() || is_comment(line) {
            continue;
        }
        if !r.env_line.is_match(line) {
            return false;
        }
        any = true;
    }
    any
}

fn redact_env(
    text: &str,
    active: &BTreeSet<Category>,
    words: &[&str],
    counts: &mut BTreeMap<Category, usize>,
) -> String {
    let r = rules();
    let mut out = String::with_capacity(text.len());
    for (i, raw) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let (line, cr) = match raw.strip_suffix('\r') {
            Some(l) => (l, "\r"),
            None => (raw, ""),
        };
        if line.trim().is_empty() || is_comment(line) {
            out.push_str(line);
        } else if let Some(c) = r.env_line.captures(line) {
            let prefix = c.get(1).map_or("", |m| m.as_str());
            let value = c.get(2).map_or("", |m| m.as_str());
            out.push_str(prefix);
            let lead = value.len() - value.trim_start().len();
            let core_end = value.trim_end().len().max(lead);
            let (lead_s, core, trail_s) = (&value[..lead], &value[lead..core_end], &value[core_end..]);
            out.push_str(lead_s);
            if active.contains(&Category::EnvValue) {
                let is_ph = r
                    .protected
                    .find(core)
                    .map_or(false, |m| m.start() == 0 && m.end() == core.len());
                if core.is_empty() || is_ph {
                    out.push_str(core);
                } else {
                    out.push_str(&Category::EnvValue.placeholder());
                    *counts.entry(Category::EnvValue).or_insert(0) += 1;
                }
            } else {
                out.push_str(&redact_fragment(core, active, words, counts));
            }
            out.push_str(trail_s);
        } else {
            out.push_str(&redact_fragment(line, active, words, counts));
        }
        out.push_str(cr);
    }
    out
}

/// Redacts `text`. Options are assumed validated via [`validate`].
pub fn redact(text: &str, opts: &RedactOptions) -> RedactOutcome {
    let mut counts: BTreeMap<Category, usize> =
        opts.categories.iter().map(|c| (*c, 0usize)).collect();

    let mut words: Vec<&str> = opts
        .custom_words
        .iter()
        .map(|w| w.as_str())
        .filter(|w| !w.is_empty())
        .collect();
    words.sort_unstable();
    words.dedup();

    let env_mode = match opts.format {
        Format::Env => true,
        Format::Text => false,
        Format::Auto => detect_env(text),
    };

    let out = if env_mode {
        redact_env(text, &opts.categories, &words, &mut counts)
    } else {
        redact_fragment(text, &opts.categories, &words, &mut counts)
    };
    RedactOutcome { text: out, counts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, opts: &RedactOptions) -> RedactOutcome {
        redact(text, opts)
    }

    #[test]
    fn env_mode_keeps_names_and_comments() {
        let o = RedactOptions::default();
        let r = run("# c\nFOO=bar\nexport KEY= v1\nEMPTY=\n", &o);
        assert_eq!(
            r.text,
            "# c\nFOO=[REDACTED:env_value]\nexport KEY= [REDACTED:env_value]\nEMPTY=\n"
        );
        assert_eq!(r.counts[&Category::EnvValue], 2);
        assert_eq!(r.counts[&Category::Jwt], 0);
    }

    #[test]
    fn text_rules_and_idempotence() {
        let mut o = RedactOptions::default();
        o.custom_words = vec!["Zed".into(), "a\nb".into()];
        let t = "mail a@b.com ip 10.0.0.1 card 4111 1111 1111 1111 Zed\nx a\nb y Bearer abcdefghij";
        let r1 = run(t, &o);
        assert!(r1.text.contains("[REDACTED:email]"));
        assert!(r1.text.contains("[REDACTED:ipv4]"));
        assert!(r1.text.contains("[REDACTED:credit_card]"));
        assert!(r1.text.contains("Bearer [REDACTED:bearer]"));
        assert_eq!(r1.text.matches('\n').count(), t.matches('\n').count());
        assert_eq!(run(&r1.text, &o).text, r1.text);
    }

    #[test]
    fn card_never_starts_inside_ipv4() {
        // Regression (Property 12): "232 2000000000000006" is Luhn-valid but
        // its first group is the last octet of the IPv4 address.
        let mut o = RedactOptions::default();
        o.categories.remove(&Category::Ipv4);
        let r = run("88.204.234.232 2000000000000006 x", &o);
        assert_eq!(r.text, "88.204.234.232 [REDACTED:credit_card] x");
    }

    #[test]
    fn luhn_rejects_and_inactive_untouched() {
        let mut o = RedactOptions::default();
        let r = run("n 4111 1111 1111 1112", &o);
        assert!(!r.text.contains("credit_card"));
        o.categories.remove(&Category::Email);
        let r = run("a@b.com", &o);
        assert_eq!(r.text, "a@b.com");
        assert!(!r.counts.contains_key(&Category::Email));
    }

    #[test]
    fn validate_limits() {
        let mut o = RedactOptions::default();
        o.custom_words = vec![String::new()];
        assert_eq!(validate(&o), Err(RedactError::EmptyWord));
        o.custom_words = vec!["a".repeat(257)];
        assert_eq!(validate(&o), Err(RedactError::WordTooLong));
        o.custom_words = vec!["a".into(); 101];
        assert_eq!(validate(&o), Err(RedactError::TooManyWords));
        o.custom_words = vec!["é".repeat(256)];
        assert_eq!(validate(&o), Ok(()));
    }
}
