// Feature: privacy-micro-apps-and-burn-workspace, Property 28: Burn single-flight
//
// N concurrent `run_or_join` calls execute the burn procedure exactly once and
// all callers receive the identical report (same `Arc`). Uses `run_with` (the
// generic entry point behind `run_or_join`) so the 5s force timer and the
// process-exit watchdog are never armed.
//
// **Validates: Requirements 10.7**
use super::gen_burn::{burn_spec, build_world, FakeBackend};
use crate::privacy::burn::BurnCoordinator;
use disposable_common::AuditReport;
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use tokio::sync::broadcast;

/// Minimal join_all: polls every future on the current task until all finish.
async fn join_all<'a, T>(futs: Vec<Pin<Box<dyn Future<Output = T> + 'a>>>) -> Vec<T> {
    let mut futs: Vec<_> = futs.into_iter().map(Some).collect();
    let mut out: Vec<Option<T>> = (0..futs.len()).map(|_| None).collect();
    std::future::poll_fn(move |cx| {
        let mut pending = false;
        for (i, slot) in futs.iter_mut().enumerate() {
            if let Some(f) = slot {
                match f.as_mut().poll(cx) {
                    Poll::Ready(v) => {
                        out[i] = Some(v);
                        *slot = None;
                    }
                    Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(out.iter_mut().map(|o| o.take().unwrap()).collect())
        }
    })
    .await
}

#[test]
fn prop_28_burn_single_flight() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    runner
        .run(&(burn_spec(), 2usize..=12), |(spec, n)| {
            rt.block_on(async {
                let (state, dir, _keep) = build_world(&spec).await;
                let backend = FakeBackend::new(&state, dir.path());
                let coord = BurnCoordinator::new();
                let (tx, mut rx) = broadcast::channel::<Arc<AuditReport>>(4);

                let futs: Vec<Pin<Box<dyn Future<Output = Arc<AuditReport>> + '_>>> = (0..n)
                    .map(|_| {
                        Box::pin(coord.run_with(&backend, Some("no"), Some(&tx)))
                            as Pin<Box<dyn Future<Output = _>>>
                    })
                    .collect();
                let reports = join_all(futs).await;

                prop_assert_eq!(reports.len(), n);
                for r in &reports {
                    prop_assert!(Arc::ptr_eq(r, &reports[0]));
                    prop_assert_eq!(&**r, &*reports[0]);
                }

                // Procedure executed once: exactly one UI event, and the report
                // reflects the real pre-burn state (a second run would see 0).
                let ev = rx.try_recv().expect("one event");
                prop_assert!(Arc::ptr_eq(&ev, &reports[0]));
                prop_assert!(rx.try_recv().is_err());
                let expected = (spec.sessions.len()
                    + spec.downloads.len()
                    + spec.mcp_ids.len()) as u64
                    + spec.file_count();
                prop_assert_eq!(reports[0].removed_items, expected);

                // A later caller still joins the same report without re-running.
                let late = coord.run_with(&backend, Some("no"), Some(&tx)).await;
                prop_assert!(Arc::ptr_eq(&late, &reports[0]));
                prop_assert!(rx.try_recv().is_err());
                prop_assert!(coord.is_burning());
                Ok(())
            })
        })
        .unwrap();
}
