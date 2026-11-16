//! proptest strategy for Markdown documents used by the round-trip property.
//!
//! 1..=5 headings (levels 1..=5, 1..=5 capitalised words) and 1..=10 single-line paragraphs
//! (5..=40 lowercase words). No `-`, `*`, `_`, `` ` ``, `#` (other than heading markers), `[`, `]`.
#![allow(dead_code)]
use proptest::collection::vec;
use proptest::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MdBlock {
    Heading { level: u8, text: String },
    Paragraph(String),
}

#[derive(Debug, Clone)]
pub(crate) struct MdDoc {
    pub blocks: Vec<MdBlock>,
}

impl MdDoc {
    /// Blocks separated by one blank line, with a trailing newline.
    pub fn render(&self) -> String {
        let parts: Vec<String> = self
            .blocks
            .iter()
            .map(|b| match b {
                MdBlock::Heading { level, text } => format!("{} {text}", "#".repeat(*level as usize)),
                MdBlock::Paragraph(t) => t.clone(),
            })
            .collect();
        format!("{}\n", parts.join("\n\n"))
    }

    pub fn headings(&self) -> Vec<(u8, &str)> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                MdBlock::Heading { level, text } => Some((*level, text.as_str())),
                _ => None,
            })
            .collect()
    }

    pub fn paragraphs(&self) -> Vec<&str> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                MdBlock::Paragraph(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }
}

fn heading_text() -> impl Strategy<Value = String> {
    vec("[A-Z][a-z]{1,9}", 1..=5).prop_map(|w| w.join(" "))
}

fn paragraph_text() -> impl Strategy<Value = String> {
    vec("[a-z]{2,10}", 5..=40).prop_map(|w| w.join(" "))
}

pub(crate) fn arb_markdown_doc() -> impl Strategy<Value = MdDoc> {
    (
        vec((1..=5u8, heading_text()), 1..=5),
        vec(paragraph_text(), 1..=10),
        vec(0..=10usize, 5),
    )
        .prop_map(|(headings, paragraphs, slots)| {
            // Heading i goes before paragraph slot[i] (clamped); headings keep their order.
            let mut at: Vec<usize> = slots.iter().take(headings.len()).map(|s| (*s).min(paragraphs.len())).collect();
            at.sort_unstable();
            let mut blocks = Vec::new();
            let mut hi = 0;
            for (pi, p) in paragraphs.iter().enumerate() {
                while hi < headings.len() && at[hi] == pi {
                    blocks.push(MdBlock::Heading { level: headings[hi].0, text: headings[hi].1.clone() });
                    hi += 1;
                }
                blocks.push(MdBlock::Paragraph(p.clone()));
            }
            for (level, text) in &headings[hi..] {
                blocks.push(MdBlock::Heading { level: *level, text: text.clone() });
            }
            MdDoc { blocks }
        })
}

#[cfg(test)]
mod self_checks {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn markdown_docs_respect_shape(d in arb_markdown_doc()) {
            prop_assert!((1..=5).contains(&d.headings().len()));
            prop_assert!((1..=10).contains(&d.paragraphs().len()));
            for (level, text) in d.headings() {
                prop_assert!((1..=5).contains(&level));
                prop_assert!((1..=5).contains(&text.split(' ').count()));
            }
            for p in d.paragraphs() {
                prop_assert!((5..=40).contains(&p.split(' ').count()));
                prop_assert!(!p.contains(['-', '*', '_', '`', '#', '[', ']', '\n']));
            }
            let r = d.render();
            prop_assert!(r.ends_with('\n'));
            prop_assert_eq!(r.matches("\n\n").count(), d.blocks.len() - 1);
        }
    }
}
