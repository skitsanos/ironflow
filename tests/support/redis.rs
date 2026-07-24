#![cfg(feature = "redis")]

use ironflow::storage::event_store::RedisEventStore;
use ironflow::storage::redis_store::RedisStateStore;
use redis::aio::ConnectionManager;
use uuid::Uuid;

pub struct RedisTest {
    pub url: String,
    pub prefix: String,
}

impl RedisTest {
    pub async fn connect(label: &str) -> Option<Self> {
        let configured_url = std::env::var("IRONFLOW_REDIS_TEST_URL").ok();
        let required = std::env::var("IRONFLOW_REDIS_TEST_REQUIRED")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"));
        let url = configured_url
            .clone()
            .unwrap_or_else(|| "redis://127.0.0.1:6379/".to_string());
        let id = Uuid::new_v4().simple().to_string();
        let fixture = Self {
            url,
            prefix: format!("ironflow_test:{label}:{id}:"),
        };

        match fixture.connection().await {
            Ok(_) => Some(fixture),
            Err(error) if configured_url.is_some() || required => {
                panic!("required Redis test server is unavailable: {error}")
            }
            Err(error) => {
                eprintln!(
                    "Skipping Redis test: set IRONFLOW_REDIS_TEST_URL to require a server ({error})"
                );
                None
            }
        }
    }

    // Each integration-test crate includes this shared module independently.
    #[allow(dead_code)]
    pub async fn state_store(&self, ttl: Option<u64>) -> RedisStateStore {
        RedisStateStore::new(&self.url, Some(self.prefix.clone()), ttl)
            .await
            .expect("Redis state test store should connect")
    }

    #[allow(dead_code)]
    pub async fn event_store(&self, ttl: Option<u64>) -> RedisEventStore {
        RedisEventStore::new(&self.url, Some(self.prefix.clone()), ttl)
            .await
            .expect("Redis event test store should connect")
    }

    pub async fn connection(&self) -> redis::RedisResult<ConnectionManager> {
        let client = redis::Client::open(self.url.as_str())?;
        ConnectionManager::new(client).await
    }

    pub async fn cleanup(&self) {
        let Ok(mut conn) = self.connection().await else {
            return;
        };
        let pattern = format!("{}*", self.prefix);
        let mut cursor = 0_u64;
        loop {
            let Ok((next, keys)) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100)
                .query_async::<(u64, Vec<String>)>(&mut conn)
                .await
            else {
                return;
            };
            if !keys.is_empty() {
                let _: redis::RedisResult<usize> =
                    redis::cmd("UNLINK").arg(keys).query_async(&mut conn).await;
            }
            cursor = next;
            if cursor == 0 {
                break;
            }
        }
    }
}
