pub(super) fn initialize_provider() {
    // The dependency graph enables both ring and AWS-LC. SQLx certificate
    // verification and Redis use Rustls' process default; preserve any provider
    // already selected by an embedding application.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    #[test]
    fn provider_initialization_is_idempotent() {
        super::initialize_provider();
        let original = rustls::crypto::CryptoProvider::get_default().unwrap();
        super::initialize_provider();
        assert!(std::sync::Arc::ptr_eq(
            original,
            rustls::crypto::CryptoProvider::get_default().unwrap()
        ));
    }
}
