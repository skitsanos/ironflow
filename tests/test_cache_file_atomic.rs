//! IF-218 (b): the `cache` file backend replaces an entry atomically.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

fn payload(tag: usize) -> serde_json::Value {
    // Large enough that a truncate-then-write spans many write calls.
    json!({"tag": tag, "body": "x".repeat(4 * 1024 * 1024)})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_concurrent_reader_never_sees_a_partial_entry() {
    let directory = tempfile::tempdir().unwrap();
    let cache_dir = directory.path().join("cache");
    let registry = Arc::new(NodeRegistry::with_builtins());
    let set = |tag: usize| {
        let registry = Arc::clone(&registry);
        let cache_dir = cache_dir.clone();
        async move {
            registry
                .get("cache_set")
                .unwrap()
                .execute(
                    &json!({"key": "shared", "value": payload(tag), "backend": "file", "cache_dir": cache_dir}),
                    &Context::new(),
                )
                .await
        }
    };
    set(0).await.unwrap();

    let done = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let registry = Arc::clone(&registry);
            let cache_dir = cache_dir.clone();
            let done = Arc::clone(&done);
            tokio::spawn(async move {
                let mut reads = 0_usize;
                while !done.load(Ordering::Relaxed) {
                    let output = registry
                        .get("cache_get")
                        .unwrap()
                        .execute(
                            &json!({"key": "shared", "backend": "file", "cache_dir": cache_dir}),
                            &Context::new(),
                        )
                        .await
                        .map_err(|error| format!("read {reads}: {error:#}"))?;
                    if output["cache_hit"] != json!(true) {
                        return Err(format!("read {reads}: entry disappeared"));
                    }
                    reads += 1;
                }
                Ok::<usize, String>(reads)
            })
        })
        .collect();

    for tag in 1..=40 {
        set(tag).await.unwrap();
    }
    done.store(true, Ordering::Relaxed);
    for reader in readers {
        reader.await.unwrap().unwrap();
    }
    // Nothing but the published entry is left in the versioned directory.
    assert_eq!(std::fs::read_dir(cache_dir.join("v1")).unwrap().count(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_entry_is_refused_and_its_target_is_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let cache_dir = directory.path().join("cache");
    let registry = NodeRegistry::with_builtins();
    let config =
        json!({"key": "linked", "value": "first", "backend": "file", "cache_dir": cache_dir});
    let node = registry.get("cache_set").unwrap();
    node.execute(&config, &Context::new()).await.unwrap();
    let entry = std::fs::read_dir(cache_dir.join("v1"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let victim = directory.path().join("victim.txt");
    std::fs::write(&victim, b"keep me").unwrap();
    std::fs::remove_file(&entry).unwrap();
    std::os::unix::fs::symlink(&victim, &entry).unwrap();

    let error = format!(
        "{:#}",
        node.execute(&config, &Context::new()).await.unwrap_err()
    );

    assert!(error.contains("symlink"), "{error}");
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");
}
