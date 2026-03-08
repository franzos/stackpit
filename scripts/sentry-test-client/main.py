#!/usr/bin/env python3
"""Generate realistic Sentry events against a stackpit instance using the real SDK.

Reads stackpit.toml to discover the ingest address, then fires off a variety of
event types: exceptions, messages, transactions, breadcrumbs, user feedback, etc.

Usage:
    pip install -r requirements.txt
    python main.py [--config ../../stackpit.toml] [--dsn DSN] [--count 50]

If --dsn is not given, a DSN is constructed from stackpit.toml's ingest_bind
using project_id=1 and a dummy key.
"""

import argparse
import os
import random
import re
import sys
import time
import threading

import sentry_sdk
from sentry_sdk import capture_exception, capture_message, start_transaction

# ---------------------------------------------------------------------------
# Config discovery
# ---------------------------------------------------------------------------

def discover_dsn(config_path, project_id=1, key="a" * 32):
    """Build a DSN from stackpit.toml's ingest_bind."""
    default_bind = "0.0.0.0:3001"
    bind = default_bind

    if os.path.isfile(config_path):
        try:
            with open(config_path) as f:
                for line in f:
                    m = re.match(r'^\s*ingest_bind\s*=\s*"([^"]+)"', line)
                    if m:
                        bind = m.group(1)
                        break
        except OSError:
            pass

    host, _, port = bind.rpartition(":")
    if host in ("0.0.0.0", "::", ""):
        host = "localhost"

    return f"http://{key}@{host}:{port}/{project_id}"


# ---------------------------------------------------------------------------
# Error scenarios
# ---------------------------------------------------------------------------

def divide_by_zero():
    """Classic ZeroDivisionError."""
    return 1 / 0


def key_error():
    """KeyError from dict access."""
    d = {"a": 1}
    return d["missing_key"]


def type_error():
    """TypeError from bad argument."""
    return len(42)


def index_error():
    """IndexError from list access."""
    items = [1, 2, 3]
    return items[99]


def value_error():
    """ValueError from int parsing."""
    return int("not_a_number")


def attribute_error():
    """AttributeError on None."""
    obj = None
    return obj.some_method()


def recursion_error():
    """RecursionError from infinite recursion."""
    def recurse():
        return recurse()
    return recurse()


def file_not_found():
    """FileNotFoundError."""
    with open("/nonexistent/path/config.yaml") as f:
        return f.read()


def runtime_error():
    """RuntimeError with a descriptive message."""
    raise RuntimeError("Worker pool exhausted: no available threads")


def timeout_error():
    """TimeoutError simulating a network call."""
    raise TimeoutError("Connection to payments.internal:8080 timed out after 30s")


def permission_error_fn():
    """PermissionError."""
    raise PermissionError("Insufficient permissions to access /admin/users")


def connection_error_fn():
    """ConnectionError simulating a failed upstream call."""
    raise ConnectionError("Failed to connect to redis://cache.internal:6379")


def assertion_error_fn():
    """AssertionError from a violated invariant."""
    balance = -50
    assert balance >= 0, f"Account balance cannot be negative: {balance}"


def unicode_error_fn():
    """UnicodeDecodeError."""
    b"\xff\xfe".decode("ascii")


def overflow_error_fn():
    """OverflowError."""
    import math
    return math.exp(1000)


ERROR_SCENARIOS = [
    divide_by_zero,
    key_error,
    type_error,
    index_error,
    value_error,
    attribute_error,
    recursion_error,
    file_not_found,
    runtime_error,
    timeout_error,
    permission_error_fn,
    connection_error_fn,
    assertion_error_fn,
    unicode_error_fn,
    overflow_error_fn,
]

# ---------------------------------------------------------------------------
# Contextual data
# ---------------------------------------------------------------------------

ENVIRONMENTS = ["production", "staging", "development", "canary"]
RELEASES = [
    "1.0.0", "1.0.1", "1.1.0", "1.2.0-beta.1", "2.0.0-rc.1",
    "2.0.0", "2.1.0", "2.1.1", "3.0.0-alpha", "3.0.0",
]
SERVER_NAMES = [
    "web-01.us-east", "web-02.us-east", "web-03.eu-west",
    "api-01.us-east", "api-02.eu-west", "worker-01.us-east",
]
USERS = [
    {"id": "u-1001", "username": "jdoe", "email": "jdoe@example.com"},
    {"id": "u-1002", "username": "jsmith", "email": "jsmith@example.com"},
    {"id": "u-1003", "username": "mgarcia", "email": "mgarcia@example.com"},
    {"id": "u-1004", "username": "ahassan", "email": "ahassan@example.com"},
    {"id": "u-1005", "username": "ytanaka", "email": "ytanaka@example.com"},
    {"id": "u-1006", "username": "psharma", "email": "psharma@example.com"},
]
TRANSACTION_NAMES = [
    "GET /api/users", "POST /api/orders", "GET /api/products/{id}",
    "POST /api/auth/login", "GET /api/search", "PUT /api/settings",
    "DELETE /api/sessions/{id}", "GET /api/dashboard", "POST /api/checkout",
    "GET /api/health", "POST /api/webhooks/stripe", "GET /api/reports/daily",
]
LOG_MESSAGES = [
    "Cache miss for user session",
    "Rate limit threshold reached",
    "Database connection pool at 90% capacity",
    "Background job completed successfully",
    "Upstream service returned 503",
    "Feature flag 'new-checkout' evaluated to true",
    "Payment processed for order #12345",
    "User password reset requested",
    "Webhook delivery failed, scheduling retry",
    "Config reload triggered by SIGHUP",
]
BREADCRUMB_CATEGORIES = ["http", "navigation", "ui.click", "console", "query", "auth"]
BREADCRUMB_MESSAGES = [
    "GET /api/users -> 200",
    "POST /api/orders -> 201",
    "navigated to /dashboard",
    "clicked button#submit",
    "SELECT * FROM users WHERE id = ?",
    "user authenticated via OAuth",
    "cache.get('session:abc123')",
    "GET /api/products -> 500",
    "navigated to /settings",
    "clicked link.logout",
]
TAG_KEYS = {
    "browser": ["Chrome 120", "Firefox 121", "Safari 17", "Edge 120"],
    "os": ["Windows 11", "macOS 14", "Ubuntu 22.04", "iOS 17"],
    "device": ["Desktop", "Mobile", "Tablet"],
    "region": ["us-east-1", "eu-west-1", "ap-south-1", "us-west-2"],
    "feature_flag": ["new-checkout", "dark-mode", "beta-search", "v2-api"],
    "tenant": ["acme-corp", "globex", "initech", "umbrella"],
}
LEVELS = ["fatal", "error", "error", "error", "warning", "warning", "info", "debug"]


def random_tags():
    """Pick 2-5 random tags."""
    count = random.randint(2, 5)
    tags = {}
    for key in random.sample(list(TAG_KEYS.keys()), min(count, len(TAG_KEYS))):
        tags[key] = random.choice(TAG_KEYS[key])
    return tags


def random_breadcrumbs(n=None):
    """Generate n breadcrumbs."""
    if n is None:
        n = random.randint(2, 8)
    crumbs = []
    for _ in range(n):
        crumbs.append({
            "category": random.choice(BREADCRUMB_CATEGORIES),
            "message": random.choice(BREADCRUMB_MESSAGES),
            "level": random.choice(["info", "warning", "error", "debug"]),
        })
    return crumbs


# ---------------------------------------------------------------------------
# Event generators
# ---------------------------------------------------------------------------

def send_exception(stats):
    """Capture a real exception with context."""
    scenario = random.choice(ERROR_SCENARIOS)

    sentry_sdk.set_user(random.choice(USERS))
    for k, v in random_tags().items():
        sentry_sdk.set_tag(k, v)
    sentry_sdk.set_context("request_info", {
        "method": random.choice(["GET", "POST", "PUT", "DELETE"]),
        "url": f"https://api.example.com{random.choice(TRANSACTION_NAMES).split(' ')[1]}",
        "status_code": random.choice([400, 500, 502, 503]),
    })

    for crumb in random_breadcrumbs():
        sentry_sdk.add_breadcrumb(
            category=crumb["category"],
            message=crumb["message"],
            level=crumb["level"],
        )

    try:
        scenario()
    except Exception:
        capture_exception()
        stats["exceptions"] += 1


def send_message(stats):
    """Capture a log-level message with tags."""
    sentry_sdk.set_user(random.choice(USERS))
    for k, v in random_tags().items():
        sentry_sdk.set_tag(k, v)

    level = random.choice(LEVELS)
    msg = random.choice(LOG_MESSAGES)
    capture_message(msg, level=level)
    stats["messages"] += 1


def send_transaction(stats):
    """Send a performance transaction with child spans."""
    tx_name = random.choice(TRANSACTION_NAMES)
    op = tx_name.split(" ")[0].lower()

    with start_transaction(op=op, name=tx_name) as tx:
        tx.set_tag("region", random.choice(TAG_KEYS["region"]))
        tx.set_tag("tenant", random.choice(TAG_KEYS["tenant"]))
        sentry_sdk.set_user(random.choice(USERS))

        # simulate 2-4 child spans
        span_ops = ["db.query", "http.client", "cache.get", "serialize", "template.render"]
        for _ in range(random.randint(2, 4)):
            span_op = random.choice(span_ops)
            with tx.start_child(op=span_op, name=f"{span_op} operation") as span:
                duration = random.uniform(0.005, 0.5)
                time.sleep(duration)
                if random.random() < 0.1:
                    span.set_status("internal_error")
                else:
                    span.set_status("ok")

        if random.random() < 0.15:
            tx.set_status("internal_error")
        else:
            tx.set_status("ok")

    stats["transactions"] += 1


# Weighted distribution: exceptions are most common, then messages, then transactions
EVENT_GENERATORS = [
    (send_exception, 50),
    (send_message, 30),
    (send_transaction, 20),
]


def pick_generator():
    funcs, weights = zip(*EVENT_GENERATORS)
    return random.choices(funcs, weights=weights, k=1)[0]


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def run(dsn, count, threads):
    sentry_sdk.init(
        dsn=dsn,
        traces_sample_rate=1.0,
        environment=random.choice(ENVIRONMENTS),
        release=random.choice(RELEASES),
        server_name=random.choice(SERVER_NAMES),
        send_default_pii=True,
        # disable the default integrations that expect a real app runtime
        default_integrations=False,
        # flush aggressively so events arrive quickly
        shutdown_timeout=10,
    )

    stats = {"exceptions": 0, "messages": 0, "transactions": 0, "errors": 0}
    lock = threading.Lock()

    def worker(n):
        for _ in range(n):
            try:
                gen = pick_generator()
                gen(stats)
            except Exception as e:
                with lock:
                    stats["errors"] += 1
                print(f"  generator error: {e}", file=sys.stderr)

    per_thread = count // threads
    remainder = count % threads
    thread_list = []

    print(f"sending {count} events via {threads} thread(s) to {dsn}")

    t0 = time.time()
    for i in range(threads):
        n = per_thread + (1 if i < remainder else 0)
        t = threading.Thread(target=worker, args=(n,), daemon=True)
        t.start()
        thread_list.append(t)

    for t in thread_list:
        t.join()

    # flush remaining events
    sentry_sdk.flush(timeout=10)
    elapsed = time.time() - t0

    print(f"done in {elapsed:.1f}s — "
          f"exceptions: {stats['exceptions']}, "
          f"messages: {stats['messages']}, "
          f"transactions: {stats['transactions']}, "
          f"errors: {stats['errors']}")


def main():
    parser = argparse.ArgumentParser(description="Generate Sentry SDK events against stackpit")
    parser.add_argument("--config", default="../../stackpit.toml",
                        help="path to stackpit.toml (default: ../../stackpit.toml)")
    parser.add_argument("--dsn", default=None,
                        help="explicit DSN (overrides config discovery)")
    parser.add_argument("--project-id", type=int, default=1,
                        help="project ID for auto-discovered DSN (default: 1)")
    parser.add_argument("--key", default="a" * 32,
                        help="sentry key for auto-discovered DSN (default: 32 'a's)")
    parser.add_argument("--count", type=int, default=50,
                        help="number of events to send (default: 50)")
    parser.add_argument("--threads", type=int, default=4,
                        help="number of sender threads (default: 4)")
    args = parser.parse_args()

    dsn = args.dsn or discover_dsn(args.config, args.project_id, args.key)
    run(dsn, args.count, args.threads)


if __name__ == "__main__":
    main()
