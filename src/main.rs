use tracing_subscriber::EnvFilter;

fn exit_with_error(error: anyhow::Error) -> ! {
    let diagnostic = ironflow::util::sensitive_url::redact_sensitive_text(&format!("{error:#}"));
    eprintln!("Error: {diagnostic}");
    std::process::exit(1);
}

fn main() {
    // SAFETY: this is the first operation in synchronous `main`; no Tokio
    // runtime, tracing subscriber, or application worker thread exists yet.
    let loaded_dotenv = match unsafe { ironflow::cli::bootstrap_environment() } {
        Ok(path) => path,
        Err(error) => exit_with_error(error),
    };

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    if let Some(path) = loaded_dotenv {
        tracing::info!(path = %path.display(), "Loaded dotenv file");
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => exit_with_error(error.into()),
    };

    if let Err(error) = runtime.block_on(ironflow::cli::run_cli()) {
        exit_with_error(error);
    }
}
