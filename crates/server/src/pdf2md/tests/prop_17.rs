// Feature: pdf-to-markdown, Property 17: Gaya teks dibungkus dengan benar
//
// **Validates: Requirements 3.6, 3.7**
//
// Text uses only alphanumerics and single spaces so the only `*` characters in the output are
// style markers. Every span except the last ends with a space, which keeps neighbouring
// marker runs unambiguous.
use proptest::collection::vec;
use proptest::prelude::*;

use crate::pdf2md::model::Span;
use crate::pdf2md::structure::inline::render_spans;

fn span(text: String, bold: bool, italic: bool) -> Span {
    Span { text, bold, italic, mono: false, size: 12 }
}

fn arb_chunk() -> impl Strategy<Value = String> {
    vec("[a-zA-Z0-9]{1,6}", 1..=3).prop_map(|w| w.join(" "))
}

/// Spans with random styles. All but the last carry a trailing space.
fn arb_styled() -> impl Strategy<Value = Vec<Span>> {
    vec((arb_chunk(), any::<bool>(), any::<bool>()), 1..=6).prop_map(|v| {
        let n = v.len();
        v.into_iter()
            .enumerate()
            .map(|(i, (t, b, it))| span(if i + 1 < n { format!("{t} ") } else { t }, b, it))
            .collect()
    })
}

/// Spans that all share one style.
fn arb_uniform() -> impl Strategy<Value = (Vec<Span>, bool, bool)> {
    (vec(arb_chunk(), 1..=5), any::<bool>(), any::<bool>()).prop_map(|(v, b, it)| {
        let n = v.len();
        let spans = v
            .into_iter()
            .enumerate()
            .map(|(i, t)| span(if i + 1 < n { format!("{t} ") } else { t }, b, it))
            .collect();
        (spans, b, it)
    })
}

fn plain(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Maximal `*` runs as (start, end) byte ranges.
fn star_runs(s: &str) -> Vec<(usize, usize)> {
    let b = s.as_bytes();
    let mut runs = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'*' {
            let st = i;
            while i < b.len() && b[i] == b'*' {
                i += 1;
            }
            runs.push((st, i));
        } else {
            i += 1;
        }
    }
    runs
}

/// Marker lengths expected from merging adjacent same-style spans.
fn expected_markers(spans: &[Span]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut prev: Option<(bool, bool)> = None;
    for s in spans {
        let st = (s.bold, s.italic);
        if prev != Some(st) {
            let len = match st {
                (true, true) => 3,
                (true, false) => 2,
                (false, true) => 1,
                _ => 0,
            };
            if len > 0 {
                out.push(len);
            }
        }
        prev = Some(st);
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn markers_are_balanced_and_text_is_preserved(spans in arb_styled()) {
        let out = render_spans(&spans, false);

        // Removing markers yields the joined plain text.
        prop_assert_eq!(out.replace('*', ""), plain(&spans));

        let runs = star_runs(&out);
        let lens: Vec<usize> = runs.iter().map(|(a, b)| b - a).collect();
        let exp: Vec<usize> = expected_markers(&spans).into_iter().flat_map(|l| [l, l]).collect();
        prop_assert_eq!(lens, exp);

        let bytes = out.as_bytes();
        for (k, (a, b)) in runs.iter().enumerate() {
            if k % 2 == 0 {
                // Opening marker: preceded by start or whitespace, followed by non-space.
                prop_assert!(*a == 0 || (bytes[a - 1] as char).is_whitespace());
                prop_assert!(*b < bytes.len() && !out[*b..].chars().next().unwrap().is_whitespace());
            } else {
                // Closing marker: preceded by non-space, followed by end or whitespace.
                prop_assert!(!out[..*a].chars().next_back().unwrap().is_whitespace());
                prop_assert!(*b == bytes.len() || (bytes[*b] as char).is_whitespace());
            }
        }
    }

    #[test]
    fn uniform_block_uses_one_pair((spans, bold, italic) in arb_uniform()) {
        let out = render_spans(&spans, false);
        prop_assert_eq!(out.replace('*', ""), plain(&spans));
        let runs = star_runs(&out);
        let marker = match (bold, italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            _ => "",
        };
        if marker.is_empty() {
            prop_assert!(runs.is_empty());
        } else {
            prop_assert_eq!(runs.len(), 2);
            prop_assert!(out.starts_with(marker) && out.ends_with(marker));
            prop_assert_eq!(runs[0], (0, marker.len()));
            prop_assert_eq!(runs[1], (out.len() - marker.len(), out.len()));
        }
    }
}
