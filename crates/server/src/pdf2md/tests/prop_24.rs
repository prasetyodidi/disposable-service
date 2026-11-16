// Feature: pdf-to-markdown, Property 24: Argumen dan lingkungan subprocess
// **Validates: Requirements 8.1, 8.2, 8.3, 8.4**
use proptest::prelude::*;

use crate::pdf2md::poppler::{pdfinfo_args, pdftohtml_args};
use std::path::Path;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// pdftohtml arguments are exactly the fixed vector; no upload name leaks in.
    #[test]
    fn pdftohtml_args_exact_and_name_free(
        from in 1u32..=100_000,
        span in 0u32..=100_000,
        upload_name in "[A-Za-z0-9_.-]{1,30}",
        dir in "[a-z0-9]{1,12}",
    ) {
        let to = from + span;
        let work = Path::new("/tmp").join(format!("w-{dir}"));
        let pdf = work.join("input.pdf");
        let args = pdftohtml_args(from, to, &pdf);
        let got: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let expected: Vec<String> = [
            "-xml", "-i", "-q", "-stdout", "-f", &from.to_string(), "-l", &to.to_string(),
            &pdf.to_string_lossy(),
        ].iter().map(|s| s.to_string()).collect();
        prop_assert_eq!(&got, &expected);
        let marker = format!("upload-{upload_name}");
        prop_assert!(got.iter().all(|a| !a.contains(&marker)));
        let info: Vec<String> = pdfinfo_args(&pdf).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        prop_assert_eq!(info, vec![pdf.to_string_lossy().into_owned()]);
    }
}

#[cfg(unix)]
mod env_check {
    use super::*;
    use crate::pdf2md::proc::{run_bounded, Deadline, ProcSpec};
    use std::ffi::OsString;
    use std::time::Duration;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// HOME and XDG_* seen by the child all live under work_dir.
        #[test]
        fn subprocess_env_is_under_work_dir(sub in "[A-Za-z0-9_-]{1,20}") {
            let base = tempfile::tempdir().unwrap();
            let work = base.path().join(&sub);
            std::fs::create_dir_all(&work).unwrap();
            let script = "printf '%s\\n%s\\n%s\\n%s\\n' \"$HOME\" \"$XDG_CACHE_HOME\" \"$XDG_CONFIG_HOME\" \"$XDG_DATA_HOME\"";
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let out = rt.block_on(run_bounded(
                &ProcSpec {
                    program: "sh",
                    args: vec![OsString::from("-c"), OsString::from(script)],
                    work_dir: &work,
                    max_stdout: 4096,
                    max_stderr: 1024,
                },
                &Deadline::new(Duration::from_secs(10)),
            )).unwrap();
            let text = String::from_utf8(out.stdout).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            prop_assert_eq!(lines.len(), 4);
            for l in lines {
                prop_assert!(!l.is_empty());
                prop_assert!(Path::new(l).starts_with(&work), "{} not under {}", l, work.display());
            }
        }
    }
}
