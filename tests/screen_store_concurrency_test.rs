//! Concurrent structural mutations must not corrupt each other. The specific
//! hazard: `create_screen` checks `dir.exists()`, then scaffolds, and on any
//! per-file failure removes the whole dir — so an interleaved pair can have
//! one call's cleanup delete the other call's finished screen.

mod common;

use std::sync::{Arc, Barrier};

use byonk::services::screen_store::StarterKind;
use common::store::build_store;

#[test]
fn test_concurrent_creates_leave_every_successful_screen_intact() {
    let tmp = tempfile::tempdir().unwrap();
    // No pre-scaffolded screens — the concurrent creates below make them.
    let store = build_store(tmp.path(), &[]);

    // Without a barrier, threads spawned in a loop can finish sequentially on
    // a loaded runner (thread 0 done before thread 7 even starts), so this
    // wouldn't actually exercise concurrent access. Release all 8 together.
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.create_screen("local", &format!("screen{i}"), StarterKind::Minimal)
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Every create targeted a distinct name, so every one must have succeeded.
    for (i, r) in results.iter().enumerate() {
        assert!(r.is_ok(), "create of screen{i} failed: {r:?}");
    }
    // And every one must still be on disk and readable afterwards.
    for i in 0..8 {
        let r = store.read_file(&format!("local/screen{i}"), "meta.yaml");
        assert!(r.is_ok(), "screen{i} missing after concurrent creates");
    }
}

#[test]
fn test_concurrent_creates_of_the_same_name_yield_exactly_one_winner() {
    let tmp = tempfile::tempdir().unwrap();
    let store = build_store(tmp.path(), &[]);

    // Same rationale as above: without a barrier this test has no
    // discriminating power — a buggy, unlocked create_screen could still
    // pass every time on a fast/idle machine simply because the threads
    // never actually overlap.
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.create_screen("local", "contended", StarterKind::Minimal)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let winners = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(winners, 1, "expected exactly one create to win");
    // The winner's screen must be complete, not half-scaffolded by a loser's cleanup.
    for f in ["meta.yaml", "script.lua", "screen.svg"] {
        assert!(
            store.read_file("local/contended", f).is_ok(),
            "{f} missing after contended create"
        );
    }
}
