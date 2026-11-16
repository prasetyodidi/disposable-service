//! Property 33: round trip Markdown -> PDF -> Markdown.
//!
//! Feature: pdf-to-markdown, Property 33: the words of every heading and paragraph of a generated
//! Markdown document survive `generate_pandoc_pdf_bytes` + `PopplerEngine` + `convert`, in the same
//! order. Words are compared as a sequence; paragraph boundaries are not guaranteed (LaTeX PDFs
//! separate paragraphs only by indentation).
//!
//! **Validates: Requirements 6.7, 16.2**
use super::gen_markdown::{arb_markdown_doc, MdBlock, MdDoc};
use super::helpers::{require_tools, skip_unless_tool};
use crate::pdf2md::config::PdfConfig;
use crate::pdf2md::convert::{convert, ConvertInput};
use crate::pdf2md::engine::ExtractionEngine;
use crate::pdf2md::poppler::PopplerEngine;
use crate::pdf2md::PdfRuntime;
use crate::routes::pandoc::generate_pandoc_pdf_bytes_in;
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use std::sync::Arc;

/// Expands U+FB00..=U+FB06 ligatures.
fn expand_ligatures(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            '\u{FB05}' | '\u{FB06}' => out.push_str("st"),
            c => out.push(c),
        }
    }
    out
}

/// Removes Markdown syntax (`#`, `*`, `\`), expands ligatures and splits on whitespace.
fn words(s: &str) -> Vec<String> {
    expand_ligatures(s)
        .replace(['#', '*', '\\'], " ")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Finds `needle` as a contiguous run in `hay[from..]`; returns the index after the run.
fn find_run(hay: &[String], needle: &[String], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from);
    }
    (from..hay.len().saturating_sub(needle.len() - 1))
        .find(|&i| hay[i..i + needle.len()] == *needle)
        .map(|i| i + needle.len())
}

fn runtime(job_root: &std::path::Path) -> PdfRuntime {
    let engine: Arc<dyn ExtractionEngine> = Arc::new(PopplerEngine::default());
    PdfRuntime {
        config: PdfConfig::defaults(),
        engine,
        job_root: job_root.to_path_buf(),
    }
}

/// Typst hyphenates long words at line ends but the hyphen glyph is not in the PDF text layer
/// (pdftohtml returns `jvdct` / `bogvp` with no `-`), so a hyphenated word cannot be told apart
/// from two words. The probe documents therefore switch Typst hyphenation off with a raw block,
/// which pandoc passes to Typst and which renders no text.
const NO_HYPHENATION: &str = "```{=typst}\n#set text(hyphenate: false)\n```\n\n";

async fn round_trip(pandoc_root: &std::path::Path, md: &str) -> Result<String, String> {
    let source = format!("{NO_HYPHENATION}{md}");
    let pdf = generate_pandoc_pdf_bytes_in(pandoc_root, &source)
        .await
        .map_err(|(s, m)| format!("pandoc failed ({s}): {m}"))?;
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let rt = runtime(root.path());
    std::fs::create_dir_all(&rt.job_root).map_err(|e| e.to_string())?;
    convert(&rt, ConvertInput { file: pdf, options_raw: None, extra_file_parts: 0 })
        .await
        .map(|c| c.markdown)
        .map_err(|e| format!("convert failed: {}", e.code_str()))
}

/// Every block (heading or paragraph) must appear whole and in source order in the output words.
fn check(doc: &MdDoc, output: &str) -> Result<(), String> {
    let hay = words(output);
    let mut cursor = 0;
    for block in &doc.blocks {
        let text = match block {
            MdBlock::Heading { text, .. } => text,
            MdBlock::Paragraph(t) => t,
        };
        let needle = words(text);
        match find_run(&hay, &needle, cursor) {
            Some(end) => cursor = end,
            None => {
                return Err(format!(
                    "block not found in order after word {cursor}: {text:?}\n--- output ---\n{output}"
                ))
            }
        }
    }
    Ok(())
}

// Feature: pdf-to-markdown, Property 33: Round trip Markdown ke PDF ke Markdown
#[test]
fn markdown_pdf_markdown_round_trip_preserves_word_sequence() {
    skip_unless_tool!("pdftohtml");
    skip_unless_tool!("pdfinfo");
    skip_unless_tool!("pandoc");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Isolated pandoc job root: burn tests running in parallel wipe the shared
    // `workspace_tmp_root()`, which would delete the job directory under pandoc/typst.
    let pandoc_root = tempfile::tempdir().unwrap();

    // Preliminary probe: skip when pandoc cannot produce a PDF (no PDF engine installed).
    let probe = "# Probe\n\nhello world from the probe document\n";
    if let Err(e) = rt.block_on(generate_pandoc_pdf_bytes_in(pandoc_root.path(), probe)) {
        if require_tools() {
            panic!("pandoc tidak dapat menghasilkan PDF: {e:?} (PDF_MD_REQUIRE_TOOLS=1)");
        }
        eprintln!("SKIP: pandoc tidak dapat menghasilkan PDF (mesin PDF tidak tersedia)");
        return;
    }

    let mut runner = TestRunner::new(Config {
        cases: 100,
        failure_persistence: None,
        ..Config::default()
    });
    runner
        .run(&arb_markdown_doc(), |doc| {
            let md = doc.render();
            let out = rt
                .block_on(round_trip(pandoc_root.path(), &md))
                .map_err(TestCaseError::fail)?;
            check(&doc, &out).map_err(|m| TestCaseError::fail(format!("{m}\n--- source ---\n{md}")))
        })
        .unwrap();
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn words_strip_markdown_and_ligatures() {
        assert_eq!(words("# **Of\u{FB01}ce**  \\x"), vec!["Office", "x"]);
    }

    #[test]
    fn find_run_requires_contiguous_ordered_words() {
        let hay: Vec<String> = "a b c d b c".split(' ').map(String::from).collect();
        let n: Vec<String> = vec!["b".into(), "c".into()];
        assert_eq!(find_run(&hay, &n, 0), Some(3));
        assert_eq!(find_run(&hay, &n, 3), Some(6));
        assert_eq!(find_run(&hay, &["a".to_string(), "c".to_string()], 0), None);
    }
}
