mod config;
mod path;

use anyhow::{Context as _, Result};
use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{ACCEPT, ALLOW, CACHE_CONTROL, LOCATION};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode, Uri};
use axum::response::Response;
use tower_http::services::{ServeDir, ServeFile};

pub use config::StaticFilesConfig;

use self::path::{DecodedPath, TargetKind};

const MAX_CACHE_CONTROL_BYTES: usize = 1024;
const X_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");

#[derive(Clone, Debug)]
pub(super) struct StaticFiles {
    root: std::path::PathBuf,
    index: String,
    directory: ServeDir,
    index_file: ServeFile,
    spa_fallback: bool,
    precompressed: bool,
    cache_control: Option<HeaderValue>,
}

impl StaticFiles {
    pub(super) async fn prepare(config: StaticFilesConfig) -> Result<Self> {
        let root = path::validated_root(&config.directory).await?;
        path::validate_index_name(&config.index)?;
        path::validated_index(&root, &config.index, config.precompressed).await?;

        let cache_control = config
            .cache_control
            .as_deref()
            .map(parse_cache_control)
            .transpose()?;
        let mut directory = ServeDir::new(&root).append_index_html_on_directories(false);
        let mut index_file = ServeFile::new(root.join(&config.index));
        if config.precompressed {
            directory = directory.precompressed_br().precompressed_gzip();
            index_file = index_file.precompressed_br().precompressed_gzip();
        }
        tracing::info!(
            directory = %root.display(),
            index = %config.index,
            spa_fallback = config.spa_fallback,
            precompressed = config.precompressed,
            "static frontend enabled"
        );

        Ok(Self {
            root,
            index: config.index,
            directory,
            index_file,
            spa_fallback: config.spa_fallback,
            precompressed: config.precompressed,
            cache_control,
        })
    }

    pub(super) async fn serve(&self, mut request: Request) -> Response {
        let decoded = match DecodedPath::parse(request.uri().path()) {
            Some(path) if !path.is_reserved() => path,
            _ => return self.decorate(not_found()),
        };
        if request.method() != Method::GET && request.method() != Method::HEAD {
            return self.decorate(method_not_allowed());
        }

        let mut missing_navigation_target = false;
        match path::inspect(&self.root, decoded.relative(), self.precompressed).await {
            TargetKind::Rejected => return self.decorate(not_found()),
            TargetKind::Missing => missing_navigation_target = true,
            TargetKind::File => {}
            TargetKind::Directory => {
                if request.uri().path() != "/" && !request.uri().path().ends_with('/') {
                    return self.decorate(slash_redirect(request.uri()));
                }
                let indexed = decoded.relative().join(&self.index);
                match path::inspect(&self.root, &indexed, self.precompressed).await {
                    TargetKind::File => {
                        if rewrite_to_index(request.uri_mut(), &self.index).is_err() {
                            return self.decorate(not_found());
                        }
                    }
                    TargetKind::Missing => missing_navigation_target = true,
                    TargetKind::Directory | TargetKind::Rejected => {
                        return self.decorate(not_found());
                    }
                }
            }
        }

        let use_spa_fallback = self.spa_fallback
            && missing_navigation_target
            && decoded.is_extensionless()
            && accepts_html(&request);
        let fallback_request = use_spa_fallback.then(|| request_parts(&request));
        let mut response = self.call_directory(request).await;
        if response.status() == StatusCode::NOT_FOUND
            && let Some((method, uri, headers)) = fallback_request
        {
            let mut request = Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .expect("a previously valid request URI remains valid");
            *request.headers_mut() = headers;
            response = self.call_index(request).await;
        }
        self.decorate(response)
    }

    async fn call_directory(&self, request: Request) -> Response {
        let mut service = self.directory.clone();
        match service.try_call(request).await {
            Ok(response) => response.map(Body::new),
            Err(error) => static_io_error(error),
        }
    }

    async fn call_index(&self, request: Request) -> Response {
        let mut service = self.index_file.clone();
        match service.try_call(request).await {
            Ok(response) => response.map(Body::new),
            Err(error) => static_io_error(error),
        }
    }

    fn decorate(&self, mut response: Response) -> Response {
        response
            .headers_mut()
            .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        if (response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED)
            && let Some(value) = &self.cache_control
        {
            response.headers_mut().insert(CACHE_CONTROL, value.clone());
        }
        response
    }
}

fn parse_cache_control(value: &str) -> Result<HeaderValue> {
    if value.trim().is_empty() || value.len() > MAX_CACHE_CONTROL_BYTES {
        anyhow::bail!("static.cache_control must contain 1..={MAX_CACHE_CONTROL_BYTES} bytes");
    }
    HeaderValue::from_str(value).context("static.cache_control must be a valid HTTP header value")
}

fn accepts_html(request: &Request) -> bool {
    request.headers().get_all(ACCEPT).iter().any(|value| {
        value
            .to_str()
            .ok()
            .is_some_and(|value| value.split(',').any(html_range_is_acceptable))
    })
}

fn html_range_is_acceptable(range: &str) -> bool {
    let mut parts = range.split(';').map(str::trim);
    let media = parts.next().unwrap_or_default();
    if !media.eq_ignore_ascii_case("text/html")
        && !media.eq_ignore_ascii_case("application/xhtml+xml")
    {
        return false;
    }
    let mut quality = 1.0_f32;
    for part in parts {
        if let Some((name, value)) = part.split_once('=')
            && name.trim().eq_ignore_ascii_case("q")
        {
            quality = match value.trim().parse::<f32>() {
                Ok(value) if (0.0..=1.0).contains(&value) => value,
                _ => return false,
            };
        }
    }
    quality > 0.0
}

fn request_parts(request: &Request) -> (Method, Uri, axum::http::HeaderMap) {
    (
        request.method().clone(),
        request.uri().clone(),
        request.headers().clone(),
    )
}

fn rewrite_to_index(uri: &mut Uri, index: &str) -> Result<()> {
    let path = format!("{}{index}", uri.path());
    *uri = uri_with_path(uri, &path)?;
    Ok(())
}

fn slash_redirect(uri: &Uri) -> Response {
    let path = format!("{}/", uri.path());
    let location = match uri.query() {
        Some(query) => format!("{path}?{query}"),
        None => path,
    };
    match HeaderValue::from_str(&location).context("redirect URI is not a valid header value") {
        Ok(location) => Response::builder()
            .status(StatusCode::TEMPORARY_REDIRECT)
            .header(LOCATION, location)
            .body(Body::empty())
            .unwrap(),
        Err(_) => not_found(),
    }
}

fn uri_with_path(uri: &Uri, path: &str) -> Result<Uri> {
    let path_and_query = match uri.query() {
        Some(query) => format!("{path}?{query}"),
        None => path.to_string(),
    };
    let mut parts = uri.clone().into_parts();
    parts.path_and_query = Some(path_and_query.parse()?);
    Ok(Uri::from_parts(parts)?)
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

fn method_not_allowed() -> Response {
    Response::builder()
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .header(ALLOW, "GET, HEAD")
        .body(Body::empty())
        .unwrap()
}

fn static_io_error(error: std::io::Error) -> Response {
    tracing::error!(error = %error, "static file service failed");
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::empty())
        .unwrap()
}

#[cfg(test)]
mod tests;
