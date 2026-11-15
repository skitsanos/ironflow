//! The network hosts a database connection URL names, for the outbound
//! destination guard (IF-212).

/// Every host the driver may connect to for `connection`, with its port.
/// SQLite and Unix-socket destinations are local files and yield nothing.
/// PostgreSQL also reads `host`, `hostaddr` and `port` from the query string,
/// and a URL without a host means the local server, so each is reported.
///
/// `default_host` is what the driver uses when the URL names none
/// (`PGHOSTADDR` or `PGHOST`); without it that is the local server.
pub(super) fn network_hosts(connection: &str, default_host: Option<String>) -> Vec<(String, u16)> {
    let Ok(url) = url::Url::parse(connection) else {
        return Vec::new();
    };
    let scheme = url.scheme().to_ascii_lowercase();
    if scheme.starts_with("sqlite") {
        return Vec::new();
    }
    let default_port = match scheme.as_str() {
        "mysql" | "mariadb" => 3306,
        _ => 5432,
    };
    let query = |name: &'static str| -> Vec<String> {
        url.query_pairs()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .collect()
    };
    let port = query("port")
        .iter()
        .filter_map(|value| value.parse::<u16>().ok())
        .next_back()
        .or(url.port())
        .unwrap_or(default_port);

    let candidates: Vec<String> = url
        .host_str()
        .filter(|host| !host.is_empty())
        .map(|host| {
            percent_encoding::percent_decode_str(host)
                .decode_utf8_lossy()
                .into_owned()
        })
        .into_iter()
        .chain(query("host"))
        .chain(query("hostaddr"))
        .collect();
    // A host that is a path is a Unix socket directory, not a network host.
    let uses_socket = candidates.iter().any(|host| host.starts_with('/'));
    let mut hosts: Vec<String> = candidates
        .into_iter()
        .filter(|host| !host.starts_with('/'))
        .collect();
    if hosts.is_empty() && !uses_socket {
        hosts.push(default_host.unwrap_or_else(|| "localhost".to_owned()));
    }
    hosts.into_iter().map(|host| (host, port)).collect()
}

#[cfg(test)]
mod tests {
    use super::network_hosts;

    fn hosts(connection: &str) -> Vec<(String, u16)> {
        network_hosts(connection, None)
    }

    fn host(name: &str, port: u16) -> (String, u16) {
        (name.to_owned(), port)
    }

    #[test]
    fn local_files_and_sockets_name_no_network_host() {
        for connection in [
            "sqlite::memory:",
            "sqlite://data/app.db",
            "sqlite:data/app.db?mode=rwc",
            "postgres:///app?host=/var/run/postgresql",
            "postgres://%2Fvar%2Frun%2Fpostgresql/app",
            "not a url",
        ] {
            assert!(hosts(connection).is_empty(), "{connection}");
        }
    }

    #[test]
    fn every_host_the_driver_may_use_is_reported() {
        assert_eq!(
            hosts("postgres://user:secret@db.internal/app"),
            vec![host("db.internal", 5432)]
        );
        assert_eq!(
            hosts("postgresql://10.0.0.5:6432/app"),
            vec![host("10.0.0.5", 6432)]
        );
        assert_eq!(
            hosts("postgres://[fd00::5]/app"),
            vec![host("[fd00::5]", 5432)]
        );
        assert_eq!(
            hosts("mysql://db.internal/app"),
            vec![host("db.internal", 3306)]
        );
        // The query string can replace the host and port the URL shows.
        assert_eq!(
            hosts("postgres://public.example/app?host=10.0.0.5&port=7000"),
            vec![host("public.example", 7000), host("10.0.0.5", 7000)]
        );
        assert_eq!(
            hosts("postgres://public.example/app?hostaddr=169.254.169.254"),
            vec![host("public.example", 5432), host("169.254.169.254", 5432)]
        );
        // No host at all is the local server.
        assert_eq!(hosts("postgres:///app"), vec![host("localhost", 5432)]);
        assert_eq!(
            network_hosts("postgres:///app", Some("db.internal".to_owned())),
            vec![host("db.internal", 5432)]
        );
    }
}
