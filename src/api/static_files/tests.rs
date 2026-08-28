use axum::body::Body;
use axum::http::header::{
    ACCEPT, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_RANGE, CONTENT_TYPE, ETAG, RANGE,
};
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;

use super::{StaticFiles, StaticFilesConfig};

fn config(directory: &std::path::Path) -> StaticFilesConfig {
    StaticFilesConfig {
        directory: directory.to_path_buf(),
        index: "shell.html".to_string(),
        spa_fallback: false,
        precompressed: true,
        cache_control: Some("public, max-age=300".to_string()),
    }
}

async fn body(response: axum::response::Response) -> axum::body::Bytes {
    response.into_body().collect().await.unwrap().to_bytes()
}

#[tokio::test]
async fn serves_indexes_ranges_validators_and_precompressed_assets() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shell.html"), "root index").unwrap();
    std::fs::write(directory.path().join("app.js"), "abcdef").unwrap();
    std::fs::write(directory.path().join("app.js.br"), b"brotli-sidecar").unwrap();
    std::fs::create_dir(directory.path().join("docs")).unwrap();
    std::fs::write(directory.path().join("docs/shell.html"), "docs index").unwrap();
    std::fs::create_dir(directory.path().join("empty")).unwrap();

    let service = StaticFiles::prepare(config(directory.path()))
        .await
        .unwrap();
    let root = service
        .serve(Request::get("/").body(Body::empty()).unwrap())
        .await;
    assert_eq!(root.status(), StatusCode::OK);
    assert!(
        root.headers()[CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert_eq!(root.headers()[CACHE_CONTROL], "public, max-age=300");
    assert_eq!(root.headers()["x-content-type-options"], "nosniff");
    assert!(root.headers().contains_key(ETAG));
    assert_eq!(body(root).await, "root index");

    let redirect = service
        .serve(Request::get("/docs?view=all").body(Body::empty()).unwrap())
        .await;
    assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(redirect.headers()["location"], "/docs/?view=all");
    let nested = service
        .serve(Request::get("/docs/").body(Body::empty()).unwrap())
        .await;
    assert_eq!(body(nested).await, "docs index");
    let empty = service
        .serve(Request::get("/empty/").body(Body::empty()).unwrap())
        .await;
    assert_eq!(empty.status(), StatusCode::NOT_FOUND);

    let partial = service
        .serve(
            Request::get("/app.js")
                .header(RANGE, "bytes=1-3")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.headers()[CONTENT_RANGE], "bytes 1-3/6");
    assert_eq!(body(partial).await, "bcd");

    let compressed = service
        .serve(
            Request::get("/app.js")
                .header("accept-encoding", "br")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(compressed.status(), StatusCode::OK);
    assert_eq!(compressed.headers()[CONTENT_ENCODING], "br");
    assert_eq!(body(compressed).await, "brotli-sidecar");

    let initial = service
        .serve(Request::get("/app.js").body(Body::empty()).unwrap())
        .await;
    let etag = initial.headers()[ETAG].clone();
    let not_modified = service
        .serve(
            Request::get("/app.js")
                .header("if-none-match", etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(not_modified.headers()[CACHE_CONTROL], "public, max-age=300");

    let head = service
        .serve(
            Request::builder()
                .method(Method::HEAD)
                .uri("/app.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(head.headers()["content-length"], "6");
    assert!(body(head).await.is_empty());
}

#[tokio::test]
async fn spa_fallback_is_limited_to_html_navigation() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shell.html"), "application shell").unwrap();
    let mut options = config(directory.path());
    options.spa_fallback = true;
    options.precompressed = false;
    let service = StaticFiles::prepare(options).await.unwrap();

    for method in [Method::GET, Method::HEAD] {
        let response = service
            .serve(
                Request::builder()
                    .method(method.clone())
                    .uri("/dashboard/settings")
                    .header(ACCEPT, "text/html,application/xhtml+xml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = body(response).await;
        if method == Method::GET {
            assert_eq!(bytes, "application shell");
        } else {
            assert!(bytes.is_empty());
        }
    }

    for (path, accept) in [
        ("/dashboard/settings", "application/json"),
        ("/dashboard/settings", "text/html;q=0"),
        ("/dashboard/settings", "text/html;q=0.0"),
        ("/dashboard/settings", "*/*"),
        ("/assets/missing.js", "text/html"),
        ("/metrics", "text/html"),
    ] {
        let response = service
            .serve(
                Request::get(path)
                    .header(ACCEPT, accept)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path} {accept}");
    }

    let post = service
        .serve(
            Request::post("/dashboard")
                .header(ACCEPT, "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(post.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(post.headers()["allow"], "GET, HEAD");
}

#[tokio::test]
async fn unsafe_paths_and_sidecars_are_not_served() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("shell.html"), "safe").unwrap();
    std::fs::write(directory.path().join("app.js"), "safe asset").unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), "outside secret").unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), directory.path().join("leak.txt")).unwrap();
        std::os::unix::fs::symlink(outside.path(), directory.path().join("app.js.br")).unwrap();
    }

    let service = StaticFiles::prepare(config(directory.path()))
        .await
        .unwrap();
    for path in ["/%2e%2e/secret.txt", "/%5csecret", "/fl%6fws/hidden"] {
        let response = service
            .serve(Request::get(path).body(Body::empty()).unwrap())
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }

    #[cfg(unix)]
    for path in ["/leak.txt", "/app.js"] {
        let response = service
            .serve(Request::get(path).body(Body::empty()).unwrap())
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn invalid_static_roots_indexes_and_headers_fail_startup() {
    let missing = tempfile::tempdir().unwrap().path().join("missing");
    let error = StaticFiles::prepare(config(&missing)).await.unwrap_err();
    assert!(error.to_string().contains("static.directory"));

    let file = tempfile::NamedTempFile::new().unwrap();
    let error = StaticFiles::prepare(config(file.path())).await.unwrap_err();
    assert!(error.to_string().contains("must be a directory"));

    let directory = tempfile::tempdir().unwrap();
    let error = StaticFiles::prepare(config(directory.path()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("static.index"));

    std::fs::write(directory.path().join("shell.html"), "safe").unwrap();
    let mut invalid_index = config(directory.path());
    invalid_index.index = "../index.html".to_string();
    assert!(
        StaticFiles::prepare(invalid_index)
            .await
            .unwrap_err()
            .to_string()
            .contains("portable")
    );

    let mut invalid_cache = config(directory.path());
    invalid_cache.cache_control = Some("public\r\nx-leak: yes".to_string());
    assert!(
        StaticFiles::prepare(invalid_cache)
            .await
            .unwrap_err()
            .to_string()
            .contains("valid HTTP header")
    );
}

#[test]
fn malformed_or_ambiguous_encoded_paths_are_rejected() {
    for path in [
        "missing-leading-slash",
        "//other-host",
        "/%",
        "/%2",
        "/%zz",
        "/a%2fb",
    ] {
        assert!(super::path::DecodedPath::parse(path).is_none(), "{path}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_static_root_fails_startup() {
    let parent = tempfile::tempdir().unwrap();
    let target = parent.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("shell.html"), "safe").unwrap();
    let link = parent.path().join("public");
    std::os::unix::fs::symlink(target, &link).unwrap();

    let error = StaticFiles::prepare(config(&link)).await.unwrap_err();
    assert!(error.to_string().contains("must not be a symlink"));

    let root = parent.path().join("root");
    std::fs::create_dir(&root).unwrap();
    std::os::unix::fs::symlink(
        parent.path().join("target/shell.html"),
        root.join("shell.html"),
    )
    .unwrap();
    let error = StaticFiles::prepare(config(&root)).await.unwrap_err();
    assert!(error.to_string().contains("non-symlink file"));
}
