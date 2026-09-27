# Common recipes for redisui. Run `just` to list them.

export RUST_BACKTRACE := "1"

# Container used by the live/integration recipes.
container := "redisui-test"
container_port := "6390"
# Scratch database. Never point this at a database you care about.
test_db := "15"
# Bound to loopback on purpose: an explicit 127.0.0.1 mapping is reliable where
# a 0.0.0.0 one intermittently stops accepting connections.
live_url := "redis://127.0.0.1:" + container_port + "/" + test_db

# Show available recipes.
default:
    @just --list

# The three gates. Same as what CI runs, minus the integration tests.
check:
    cargo fmt --check
    cargo clippy --all-targets
    cargo test

# Reformat instead of failing.
fmt:
    cargo fmt

# Format, then run every gate.
fix: fmt
    cargo clippy --all-targets
    cargo test

# Unit + render tests only. No Redis needed.
test:
    cargo test

# Same, via nextest, for per-test failure output.
test-fast:
    cargo nextest run --hide-progress-bar --failure-output final

# Start the throwaway Redis this repo develops against.
#
# Deliberately does not probe the published port before starting: Docker Desktop
# can leave a stale mapping that `nc -z` reports as closed while connections
# still work, and recreating the container would throw away the seeded demo
# data. If a mapping really is stale, `just container-down container-up` forces
# it, and re-run `just seed`.
container-up:
    @if docker inspect {{container}} >/dev/null 2>&1; then \
        docker start {{container}} >/dev/null 2>&1 || true; \
    else \
        docker run -d --name {{container}} -p 127.0.0.1:{{container_port}}:6379 redis:7-alpine >/dev/null; \
    fi
    @echo "redis on 127.0.0.1:{{container_port}} (db {{test_db}} is scratch)"

container-down:
    -docker rm -f {{container}}

container-logs:
    docker logs --tail 50 {{container}}

# Seed demo data covering every supported type, into database 0.
seed: container-up
    docker exec {{container}} redis-cli FLUSHALL >/dev/null
    docker exec {{container}} redis-cli SET greeting "hello redisui" >/dev/null
    docker exec {{container}} redis-cli SET counter:1 "1000" >/dev/null
    docker exec {{container}} redis-cli SETEX session:abc 300 "temp-token" >/dev/null
    printf '\xff\xfe\x00\x01\x41' | docker exec -i {{container}} redis-cli -x SET bin:raw >/dev/null
    docker exec {{container}} redis-cli RPUSH queue:jobs a b c d e >/dev/null
    docker exec {{container}} redis-cli HSET user:1 name alice email a@b.c age 31 >/dev/null
    docker exec {{container}} redis-cli HSET user:2 name bob email b@c.d >/dev/null
    docker exec {{container}} redis-cli SADD tags red green blue >/dev/null
    docker exec {{container}} redis-cli ZADD leaderboard 1500 alice 2100 bob 900 carol >/dev/null
    @echo "seeded $(docker exec {{container}} redis-cli dbsize) keys in db 0"

# Integration tests against the live container. Starts it if needed.
#
# The preflight runs one cheap test first so an unreachable server is one clear
# line rather than nineteen identical panics.
test-live: container-up
    @REDISUI_TEST_REDIS={{live_url}} cargo nextest run --test live_store \
        -E 'test(connect_reports_a_redis_version)' --hide-progress-bar >/dev/null 2>&1 \
        || { echo ""; \
             echo "cannot reach Redis at {{live_url}}"; \
             echo "  - is the container up?  just container-logs"; \
             echo "  - docker prints no published port?  just container-down container-up"; \
             echo "  - Docker Desktop port forwarding can go stale; a fresh container fixes it"; \
             exit 1; }
    REDISUI_TEST_REDIS={{live_url}} cargo test --test live_store

# Integration tests plus the authentication scenario.
test-auth: container-up
    REDISUI_TEST_REDIS={{live_url}} REDISUI_TEST_PASSWORD=just-smoke-test cargo test --test live_store

# Render the TUI headlessly and print the screen it drew.
# Prefers a vendored copy, then the global tui-smoke-test skill.
smoke: container-up
    @script="{{justfile_directory()}}/tools/drive_tui.py"; \
    if [ ! -f "$script" ]; then \
        script="$HOME/.config/opencode/skills/tui-smoke-test/scripts/drive_tui.py"; \
    fi; \
    if [ ! -f "$script" ]; then \
        echo "drive_tui.py not found; see the tui-smoke-test skill (needs pyte)"; exit 1; \
    fi; \
    "$script" --cols 100 --rows 30 --require connected -- \
        ./target/debug/redisui --url {{live_url}}

# Dependency policy: advisories, licenses, duplicate and source rules.
deny:
    cargo deny check

# Everything CI runs, locally.
ci: check test-live
    @echo "all gates green"

# Serve the docs locally.
doc:
    cargo doc --no-deps --open
