use axum::body::Body;
use axum::http::{Method, Request, StatusCode};

use super::{StaticFiles, body, config};

async fn request(
    service: &StaticFiles,
    path: &str,
    method: Method,
    encoding: &str,
) -> axum::response::Response {
    service
        .serve(
            Request::builder()
                .method(method)
                .uri(path)
                .header("accept", "text/html")
                .header("accept-encoding", encoding)
                .body(Body::empty())
                .unwrap(),
        )
        .await
}

#[cfg(unix)]
async fn assert_rejected(service: &StaticFiles, paths: &[&str]) {
    for method in [Method::GET, Method::HEAD] {
        for path in paths {
            for encoding in ["gzip", "br", "identity"] {
                let response = request(service, path, method.clone(), encoding).await;
                assert_eq!(
                    response.status(),
                    StatusCode::NOT_FOUND,
                    "{method} {path} {encoding}"
                );
                assert_eq!(response.headers()["x-content-type-options"], "nosniff");
                assert!(!response.headers().contains_key("content-encoding"));
                assert!(body(response).await.is_empty());
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn orphan_sidecars_cannot_escape_or_resolve_dangling_links() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shell.html"), "safe shell").unwrap();
    std::fs::write(outside.path().join("secret"), "outside sentinel").unwrap();
    for suffix in ["gz", "br"] {
        for (name, target) in [("orphan", "secret"), ("dangling", "absent")] {
            std::os::unix::fs::symlink(
                outside.path().join(target),
                directory.path().join(format!("{name}.{suffix}")),
            )
            .unwrap();
        }
    }
    let mut options = config(directory.path());
    options.spa_fallback = true;
    let service = StaticFiles::prepare(options).await.unwrap();
    assert_rejected(&service, &["/orphan", "/dangling"]).await;
}

#[cfg(unix)]
#[tokio::test]
async fn orphan_sidecars_under_escaping_ancestors_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shell.html"), "safe shell").unwrap();
    for suffix in ["gz", "br"] {
        std::fs::write(
            outside.path().join(format!("missing.js.{suffix}")),
            "outside sentinel",
        )
        .unwrap();
    }
    std::os::unix::fs::symlink(outside.path(), directory.path().join("escape")).unwrap();
    let service = StaticFiles::prepare(config(directory.path()))
        .await
        .unwrap();
    assert_rejected(&service, &["/escape/missing.js"]).await;
}

#[cfg(unix)]
#[tokio::test]
async fn spa_and_directory_indexes_are_rechecked_after_startup() {
    for replacement in ["shell.html", "shell.html.gz", "shell.html.br"] {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(directory.path().join("shell.html"), "safe shell").unwrap();
        std::fs::write(outside.path(), "outside sentinel").unwrap();
        let mut options = config(directory.path());
        options.spa_fallback = true;
        let service = StaticFiles::prepare(options).await.unwrap();
        let replacement = directory.path().join(replacement);
        if replacement.exists() {
            std::fs::remove_file(&replacement).unwrap();
        }
        std::os::unix::fs::symlink(outside.path(), replacement).unwrap();
        assert_rejected(&service, &["/", "/shell.html", "/dashboard/settings"]).await;
    }
}

#[tokio::test]
async fn confined_orphan_sidecars_require_the_original_file() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shell.html"), "safe shell").unwrap();
    std::fs::write(directory.path().join("orphan.js.gz"), "gzip-sidecar").unwrap();
    std::fs::write(directory.path().join("orphan.js.br"), "brotli-sidecar").unwrap();
    for precompressed in [true, false] {
        let mut options = config(directory.path());
        options.precompressed = precompressed;
        let service = StaticFiles::prepare(options).await.unwrap();
        for method in [Method::GET, Method::HEAD] {
            for encoding in ["gzip", "br", "identity"] {
                let response = request(&service, "/orphan.js", method.clone(), encoding).await;
                assert_eq!(response.status(), StatusCode::NOT_FOUND);
                assert!(!response.headers().contains_key("content-encoding"));
                assert!(body(response).await.is_empty());
            }
        }
    }
}
