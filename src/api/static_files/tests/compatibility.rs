use std::io::{Error, ErrorKind};

use axum::body::Body;
use axum::http::header::{ACCEPT, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_RANGE, CONTENT_TYPE};
use axum::http::{Method, Request, StatusCode};

use super::{StaticFiles, body, config};

async fn assert_not_found(response: axum::response::Response) {
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(!response.headers().contains_key(CACHE_CONTROL));
    assert!(!response.headers().contains_key(CONTENT_ENCODING));
    assert!(body(response).await.is_empty());
}

#[tokio::test]
async fn missing_assets_and_non_navigation_requests_remain_not_found() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shell.html"), "application shell").unwrap();
    std::fs::write(directory.path().join("app.js"), "asset").unwrap();

    for spa_fallback in [false, true] {
        let mut options = config(directory.path());
        options.spa_fallback = spa_fallback;
        let service = StaticFiles::prepare(options).await.unwrap();
        for method in [Method::GET, Method::HEAD] {
            for (path, accept) in [
                ("/missing.js", "text/html"),
                ("/assets/missing.css", "text/html"),
                ("/missing.png", "text/html"),
                ("/app.js/child", "text/html"),
                ("/dashboard", "application/json"),
            ] {
                let response = service
                    .serve(
                        Request::builder()
                            .method(method.clone())
                            .uri(path)
                            .header(ACCEPT, accept)
                            .header("accept-encoding", "br, gzip")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await;
                assert_not_found(response).await;
            }
        }
    }
}

#[tokio::test]
async fn index_removed_after_startup_remains_not_found() {
    let directory = tempfile::tempdir().unwrap();
    let index = directory.path().join("shell.html");
    std::fs::write(&index, "application shell").unwrap();
    let mut options = config(directory.path());
    options.spa_fallback = true;
    let service = StaticFiles::prepare(options).await.unwrap();
    std::fs::remove_file(index).unwrap();

    for method in [Method::GET, Method::HEAD] {
        for path in ["/", "/shell.html", "/dashboard"] {
            let response = service
                .serve(
                    Request::builder()
                        .method(method.clone())
                        .uri(path)
                        .header(ACCEPT, "text/html")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await;
            assert_not_found(response).await;
        }
    }
}

#[tokio::test]
async fn io_errors_preserve_not_found_and_internal_error_boundaries() {
    for (kind, expected) in [
        (ErrorKind::NotFound, StatusCode::NOT_FOUND),
        (ErrorKind::PermissionDenied, StatusCode::NOT_FOUND),
        (ErrorKind::NotADirectory, StatusCode::NOT_FOUND),
        (ErrorKind::Other, StatusCode::INTERNAL_SERVER_ERROR),
        (ErrorKind::Interrupted, StatusCode::INTERNAL_SERVER_ERROR),
    ] {
        let response = super::super::static_io_error(Error::from(kind));
        assert_eq!(response.status(), expected, "{kind:?}");
        assert!(body(response).await.is_empty());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_assets_sidecars_and_spa_index_remain_not_found() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    for name in ["shell.html", "asset", "asset.br"] {
        std::fs::write(directory.path().join(name), "fixture").unwrap();
    }
    let mut options = config(directory.path());
    options.spa_fallback = true;
    let service = StaticFiles::prepare(options).await.unwrap();

    for (name, path, encoding) in [
        ("asset", "/asset", "identity"),
        ("asset.br", "/asset", "br"),
        ("shell.html", "/dashboard", "identity"),
    ] {
        let file = directory.path().join(name);
        let permissions = std::fs::metadata(&file).unwrap().permissions();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::File::open(&file).is_ok() {
            std::fs::set_permissions(&file, permissions).unwrap();
            eprintln!("permission checks skipped: process can read mode-000 files");
            return;
        }
        let response = service
            .serve(
                Request::get(path)
                    .header(ACCEPT, "text/html")
                    .header("accept-encoding", encoding)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        std::fs::set_permissions(&file, permissions).unwrap();
        assert_not_found(response).await;
    }
}

#[tokio::test]
async fn asset_negotiation_and_invalid_ranges_preserve_response_contracts() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shell.html"), "application shell").unwrap();
    std::fs::write(directory.path().join("app.js"), "abcdef").unwrap();
    std::fs::write(directory.path().join("app.js.gz"), "gzip-sidecar").unwrap();
    let service = StaticFiles::prepare(config(directory.path()))
        .await
        .unwrap();

    for (encoding, expected) in [("gzip", "gzip-sidecar"), ("br", "abcdef")] {
        for method in [Method::GET, Method::HEAD] {
            let response = service
                .serve(
                    Request::builder()
                        .method(method.clone())
                        .uri("/app.js")
                        .header("accept-encoding", encoding)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[CONTENT_TYPE], "text/javascript");
            assert_eq!(
                response.headers()["content-length"],
                expected.len().to_string()
            );
            assert_eq!(response.headers()["x-content-type-options"], "nosniff");
            assert_eq!(response.headers()[CACHE_CONTROL], "public, max-age=300");
            if encoding == "gzip" {
                assert_eq!(response.headers()[CONTENT_ENCODING], "gzip");
            } else {
                assert!(!response.headers().contains_key(CONTENT_ENCODING));
            }
            let bytes = body(response).await;
            if method == Method::HEAD {
                assert!(bytes.is_empty());
            } else {
                assert_eq!(bytes, expected);
            }
        }
    }

    for range in ["bytes=100-200", "bytes=invalid", "bytes=0-1,3-4"] {
        let response = service
            .serve(
                Request::get("/app.js")
                    .header("accept-encoding", "gzip")
                    .header("range", range)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers()[CONTENT_RANGE], "bytes */12");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert!(!response.headers().contains_key(CACHE_CONTROL));
        assert!(!response.headers().contains_key(CONTENT_ENCODING));
        assert!(!response.headers().contains_key(CONTENT_TYPE));
    }
}
