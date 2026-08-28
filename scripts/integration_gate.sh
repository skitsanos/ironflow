#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

redis_container=""
postgres_container=""

clean_workspace_artifacts() {
  cargo clean --package ironflow
}

cleanup() {
  gate_status=$?
  trap - EXIT INT TERM

  if [[ -n "$redis_container" || -n "$postgres_container" ]]; then
    docker rm -f "$redis_container" "$postgres_container" >/dev/null 2>&1 || true
  fi

  echo "[integration] removing gate-owned IronFlow artifacts"
  if ! clean_workspace_artifacts; then
    echo "Warning: could not remove gate-owned IronFlow artifacts." >&2
  fi

  exit "$gate_status"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command in cargo cargo-audit python3 bun actionlint docker; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Integration gate requires '$command' on PATH." >&2
    exit 1
  fi
done

echo "[integration] pruning stale IronFlow artifacts"
clean_workspace_artifacts
export CARGO_INCREMENTAL=0

echo "[integration] formatting and repository policies"
cargo fmt --all -- --check
git diff --check
python3 -B -m unittest discover -s scripts/tests -p 'test_*.py' -v
python3 -B scripts/check_module_size.py
bun run scripts/validate_skills.ts
bun run scripts/issues_registry.ts check
bun test scripts/tests/*.test.ts
python3 -B -m unittest discover -s .codex/hooks/tests -p 'test_*.py' -v
actionlint .github/workflows/*.yml

echo "[integration] default Rust gates"
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test --doc
cargo audit --deny warnings

echo "[integration] reclaiming default Rust artifacts"
clean_workspace_artifacts

echo "[integration] feature-enabled Rust gates"
cargo check --all-targets --features postgres,redis
cargo clippy --all-targets --features postgres,redis -- -D warnings

echo "[integration] release build and Lua examples"
cargo build --release
example_count=0
example_failures=0
while IFS= read -r -d '' flow; do
  example_count=$((example_count + 1))
  if ! ./target/release/ironflow validate "$flow"; then
    echo "Example validation failed: $flow" >&2
    example_failures=$((example_failures + 1))
  fi
done < <(find examples -type f -name '*.lua' -print0)
if (( example_failures > 0 )); then
  echo "$example_failures of $example_count Lua examples failed validation." >&2
  exit 1
fi
echo "Validated $example_count Lua examples."

echo "[integration] reclaiming pre-integration Rust artifacts"
clean_workspace_artifacts

container_suffix="$$"
redis_container="ironflow-integration-redis-$container_suffix"
postgres_container="ironflow-integration-postgres-$container_suffix"
postgres_password=$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')

echo "[integration] disposable Redis and PostgreSQL"
docker pull redis:latest
docker pull postgres:latest
docker run -d --name "$redis_container" -p 127.0.0.1::6379 redis:latest >/dev/null
docker run -d --name "$postgres_container" \
  -e POSTGRES_DB=ironflow_test \
  -e POSTGRES_USER=postgres \
  -e "POSTGRES_PASSWORD=$postgres_password" \
  -p 127.0.0.1::5432 postgres:latest >/dev/null

for _ in {1..60}; do
  if docker exec "$redis_container" redis-cli ping 2>/dev/null | grep -q PONG; then break; fi
  sleep 1
done
if ! docker exec "$redis_container" redis-cli ping 2>/dev/null | grep -q PONG; then
  echo "Disposable Redis did not become ready." >&2
  exit 1
fi

for _ in {1..60}; do
  if docker exec "$postgres_container" pg_isready -U postgres -d ironflow_test >/dev/null 2>&1; then break; fi
  sleep 1
done
if ! docker exec "$postgres_container" pg_isready -U postgres -d ironflow_test >/dev/null 2>&1; then
  echo "Disposable PostgreSQL did not become ready." >&2
  exit 1
fi

redis_port=$(docker port "$redis_container" 6379/tcp | awk -F: 'NR == 1 { print $NF }')
postgres_port=$(docker port "$postgres_container" 5432/tcp | awk -F: 'NR == 1 { print $NF }')
if [[ -z "$redis_port" || -z "$postgres_port" ]]; then
  echo "Could not determine disposable storage ports." >&2
  exit 1
fi

IRONFLOW_REDIS_TEST_URL="redis://127.0.0.1:$redis_port" \
IRONFLOW_REDIS_TEST_REQUIRED=1 \
IRONFLOW_POSTGRES_TEST_REQUIRED=1 \
DATABASE_URL="postgres://postgres:$postgres_password@127.0.0.1:$postgres_port/ironflow_test" \
  cargo test --all-targets --features postgres,redis -- --test-threads=1

echo "IronFlow full integration gate passed."
