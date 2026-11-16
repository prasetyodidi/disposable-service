//! Removal of Page_Number_Line and Repeated_Line (running headers/footers).
//!
//! Only lines inside the top or bottom 10% of the page are ever removed.

use std::collections::{HashMap, HashSet};

use crate::pdf2md::model::{Page, TextLine};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Zone {
    Top,
    Bottom,
}

/// Top when `y * 10 < page_h`; Bottom when `(y + h) * 10 > page_h * 9`. Top wins when both match.
fn zone(line: &TextLine, page_h: i32) -> Option<Zone> {
    let ph = page_h as i64;
    let y = line.y as i64;
    let bottom = y + line.h as i64;
    if y * 10 < ph {
        Some(Zone::Top)
    } else if bottom * 10 > ph * 9 {
        Some(Zone::Bottom)
    } else {
        None
    }
}

/// Every run of ASCII digits becomes a single `#`; whitespace is collapsed and trimmed.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_digits = false;
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            in_digits = false;
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            in_digits = false;
            out.push(c);
        }
    }
    out
}

fn is_digits(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit())
}

/// Valid Roman numeral grammar over an already case-normalized (lowercase) string.
fn is_roman(s: &str) -> bool {
    if s.is_empty() || s.len() > 8 {
        return false;
    }
    let b = s.as_bytes();
    let mut i = 0;
    // Consume a prefix matching `count` repeats of `ch` (at most `max`).
    fn run(b: &[u8], i: &mut usize, ch: u8, max: usize) {
        let mut n = 0;
        while n < max && *i < b.len() && b[*i] == ch {
            *i += 1;
            n += 1;
        }
    }
    fn pair(b: &[u8], i: &mut usize, p: &[u8; 2]) -> bool {
        if b.len() >= *i + 2 && &b[*i..*i + 2] == p {
            *i += 2;
            true
        } else {
            false
        }
    }
    run(b, &mut i, b'm', 3);
    for (nine, four, five, one) in [
        (b"cm", b"cd", b'd', b'c'),
        (b"xc", b"xl", b'l', b'x'),
        (b"ix", b"iv", b'v', b'i'),
    ] {
        if pair(b, &mut i, nine) || pair(b, &mut i, four) {
            continue;
        }
        run(b, &mut i, five, 1);
        run(b, &mut i, one, 3);
    }
    i == b.len()
}

/// True for decimal numbers, Roman numerals, "Page N" / "Halaman N" and "N of M".
pub fn is_page_number_line(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if is_digits(t, 6) {
        return true;
    }
    if t.bytes().all(|b| b.is_ascii_alphabetic() && b"ivxlcdmIVXLCDM".contains(&b)) {
        let lower = t.to_ascii_lowercase();
        let uniform = t == lower || t == t.to_ascii_uppercase();
        if uniform && is_roman(&lower) {
            return true;
        }
    }
    let mut parts = t.split_whitespace();
    let (first, second, third, fourth) = (parts.next(), parts.next(), parts.next(), parts.next());
    match (first, second, third, fourth) {
        (Some(w), Some(n), None, None) => {
            (w.eq_ignore_ascii_case("page") || w.eq_ignore_ascii_case("halaman")) && is_digits(n, 6)
        }
        (Some(n), Some(of), Some(m), None) => {
            is_digits(n, 6) && of.eq_ignore_ascii_case("of") && is_digits(m, 6)
        }
        _ => false,
    }
}

/// Removes page-number and repeated header/footer lines. Returns the number of removed lines.
pub fn strip(pages: &mut [Page]) -> u32 {
    let total = pages.len();

    // Distinct pages per (zone, normalized text) key.
    let mut seen: HashMap<(Zone, String), HashSet<usize>> = HashMap::new();
    if total >= 3 {
        for (pi, page) in pages.iter().enumerate() {
            for line in &page.lines {
                if let Some(z) = zone(line, page.height) {
                    let key = normalize(&line.text());
                    if !key.is_empty() {
                        seen.entry((z, key)).or_default().insert(pi);
                    }
                }
            }
        }
    }
    let repeated: HashSet<(Zone, String)> = seen
        .into_iter()
        .filter(|(_, set)| set.len() >= 3 && set.len() * 2 >= total)
        .map(|(k, _)| k)
        .collect();

    let mut removed = 0u32;
    for page in pages.iter_mut() {
        let h = page.height;
        let before = page.lines.len();
        page.lines.retain(|line| {
            let Some(z) = zone(line, h) else { return true };
            let text = line.text();
            if is_page_number_line(&text) {
                return false;
            }
            !(total >= 3 && repeated.contains(&(z, normalize(&text))))
        });
        removed += (before - page.lines.len()) as u32;
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf2md::model::Span;

    fn line(text: &str, y: i32) -> TextLine {
        let s = Span { text: text.into(), bold: false, italic: false, mono: false, size: 10 };
        TextLine::new(vec![s], 10, 0, y, 100, 10)
    }

    fn page(n: u32, lines: Vec<TextLine>) -> Page {
        Page { number: n, width: 600, height: 1000, lines }
    }

    #[test]
    fn normalize_digits_and_spaces() {
        assert_eq!(normalize("  Laporan 2024 -  3 "), "Laporan # - #");
    }

    #[test]
    fn page_number_patterns() {
        for t in ["7", "123456", "iv", "XIV", "Page 3", "halaman 12", "3 of 10", "3 OF 10"] {
            assert!(is_page_number_line(t), "{t}");
        }
        for t in ["", "1234567", "iiii", "Iv", "Page", "Page 3 x", "3 of", "abc", "mmmm", "Chapter 1"] {
            assert!(!is_page_number_line(t), "{t}");
        }
    }

    #[test]
    fn removes_page_numbers_in_zone_for_any_page_count() {
        let mut pages = vec![page(1, vec![line("Body", 500), line("1", 950)])];
        assert_eq!(strip(&mut pages), 1);
        assert_eq!(pages[0].lines.len(), 1);
    }

    #[test]
    fn content_area_lines_are_never_removed() {
        let mut pages: Vec<Page> =
            (1..=4).map(|n| page(n, vec![line("5", 500), line("Same", 500)])).collect();
        assert_eq!(strip(&mut pages), 0);
    }

    #[test]
    fn repeated_lines_need_three_pages_and_half() {
        let mk = |n: u32, hdr: bool| {
            let mut l = vec![line("Body", 500)];
            if hdr {
                l.push(line(&format!("Laporan 2024 - {n}"), 10));
            }
            page(n, l)
        };
        let mut pages: Vec<Page> = (1..=4).map(|n| mk(n, n <= 3)).collect();
        assert_eq!(strip(&mut pages), 3);
        // Only 2 of 4 pages: below 3 pages, kept.
        let mut pages: Vec<Page> = (1..=4).map(|n| mk(n, n <= 2)).collect();
        assert_eq!(strip(&mut pages), 0);
        // 3 of 7 pages: under 50%, kept.
        let mut pages: Vec<Page> = (1..=7).map(|n| mk(n, n <= 3)).collect();
        assert_eq!(strip(&mut pages), 0);
        // Fewer than 3 pages: never repeated.
        let mut pages: Vec<Page> = (1..=2).map(|n| mk(n, true)).collect();
        assert_eq!(strip(&mut pages), 0);
    }
}
