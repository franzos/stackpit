#!/usr/bin/env python3
"""Generate fake Sentry data against a stackpit instance.

Reads stackpit.toml (if present) to discover the bind address.

Usage:
    python scripts/generate-fake-data.py [--config stackpit.toml] [--count 10000]
"""

import argparse
import json
import os
import random
import re
import string
import sys
import threading
import time
import urllib.request
import urllib.error
import uuid
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone

# Optional: urllib3 for connection pooling
try:
    import urllib3
    _POOL_MANAGER = None
    _HAS_URLLIB3 = True
except ImportError:
    _HAS_URLLIB3 = False

# ---------------------------------------------------------------------------
# Infrastructure: send, envelope, config discovery
# ---------------------------------------------------------------------------

def rand_hex(n):
    return "".join(random.choices("0123456789abcdef", k=n))


def rand_event_id():
    return rand_hex(32)


def rand_timestamp():
    """Random timestamp within the last 7 days."""
    now = int(time.time())
    return now - random.randint(0, 7 * 86400)


def iso_now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")


def build_envelope(header, items):
    """Build a Sentry envelope from header dict and list of (item_header, payload) tuples."""
    parts = [json.dumps(header, separators=(",", ":"))]
    for item_header, payload in items:
        payload_bytes = payload if isinstance(payload, bytes) else payload.encode()
        item_header["length"] = len(payload_bytes)
        parts.append(json.dumps(item_header, separators=(",", ":")))
        parts.append(payload_bytes.decode() if isinstance(payload_bytes, bytes) else payload_bytes)
    return "\n".join(parts) + "\n"


def _init_pool_manager(base_url):
    global _POOL_MANAGER
    if _HAS_URLLIB3 and _POOL_MANAGER is None:
        _POOL_MANAGER = urllib3.PoolManager(
            num_pools=10,
            maxsize=64,
            retries=urllib3.Retry(total=1, backoff_factor=0.1),
            timeout=urllib3.Timeout(connect=5.0, read=10.0),
        )


def send(url, body, sentry_key, content_type="application/x-sentry-envelope"):
    data = body.encode() if isinstance(body, str) else body
    auth_header = f"Sentry sentry_key={sentry_key}, sentry_version=7"

    if _HAS_URLLIB3 and _POOL_MANAGER is not None:
        try:
            resp = _POOL_MANAGER.request(
                "POST", url, body=data,
                headers={"Content-Type": content_type, "X-Sentry-Auth": auth_header},
            )
            return resp.status, resp.data.decode()
        except Exception as e:
            return 0, str(e)
    else:
        req = urllib.request.Request(url, data=data, method="POST")
        req.add_header("Content-Type", content_type)
        req.add_header("X-Sentry-Auth", auth_header)
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                return resp.status, resp.read().decode()
        except urllib.error.HTTPError as e:
            return e.code, e.read().decode()
        except Exception as e:
            return 0, str(e)


def discover_base_url(config_path):
    """Read ingest_bind address from stackpit.toml, fall back to default."""
    default = "0.0.0.0:3001"
    bind = default

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
    return f"http://{host}:{port}"


# ---------------------------------------------------------------------------
# Project definitions: ~500 projects generated from prefix-suffix combos
# ---------------------------------------------------------------------------

PREFIXES = [
    "checkout", "admin", "marketing", "customer", "docs", "design", "onboarding",
    "user", "billing", "analytics", "auth", "report", "data", "ml",
    "gateway", "notification", "search", "realtime", "upload", "webhook",
    "inventory", "support", "cms", "scheduling", "hr",
    "payment", "order", "catalog", "shipping", "fraud", "audit",
    "config", "ratelimit", "image", "queue", "health", "event",
    "storefront", "affiliate", "email", "ticket", "blog",
    "ios", "watch", "log", "metrics", "proxy", "cache", "ingestion",
    "compliance", "crm", "erp", "fleet", "geo", "iot", "kyc",
    "ledger", "marketplace", "oauth", "payout", "referral", "sso",
    "tax", "vendor", "warehouse", "workflow", "feedback", "survey",
    "ab-test", "cdn", "dns", "edge", "backup", "migration",
    "sandbox", "tenant", "token", "trace", "vault", "video",
    "voice", "chat", "calendar", "digest", "export", "import",
    "localization", "permission", "quota", "replay", "snapshot",
    "subscription", "template", "transform", "validate", "whitelist",
]

SUFFIXES = [
    "api", "service", "worker", "frontend", "backend", "app", "portal",
    "engine", "processor", "handler", "manager", "gateway", "proxy",
    "pipeline", "collector", "aggregator", "scheduler", "daemon",
]

PLATFORMS = ["javascript", "python", "node", "ruby", "java", "go", "php", "swift", "rust"]

# Platform distribution weights for project generation
_PLATFORM_WEIGHTS = {
    "javascript": 18, "python": 16, "node": 14, "ruby": 10, "java": 12,
    "go": 10, "php": 8, "swift": 5, "rust": 7,
}

def _make_projects():
    rng = random.Random(42)
    all_combos = []
    for p in PREFIXES:
        for s in SUFFIXES:
            all_combos.append(f"{p}-{s}")
    rng.shuffle(all_combos)
    combos = all_combos[:100]

    platforms = list(_PLATFORM_WEIGHTS.keys())
    weights = [_PLATFORM_WEIGHTS[p] for p in platforms]

    projects = []
    for i, name in enumerate(combos, start=1):
        platform = rng.choices(platforms, weights=weights)[0]
        key = rand_hex(32)
        projects.append({"id": i, "key": key, "name": name, "platform": platform})
    return projects


PROJECTS = _make_projects()

# ---------------------------------------------------------------------------
# SDK info per platform
# ---------------------------------------------------------------------------

SDK_BY_PLATFORM = {
    "javascript": ("sentry.javascript.browser", "8.42.0"),
    "python":     ("sentry.python", "2.19.2"),
    "node":       ("sentry.javascript.node", "8.42.0"),
    "ruby":       ("sentry.ruby", "5.22.1"),
    "java":       ("sentry.java", "7.20.0"),
    "go":         ("sentry.go", "0.31.1"),
    "php":        ("sentry.php", "4.10.0"),
    "swift":      ("sentry.cocoa", "8.44.0"),
    "rust":       ("sentry.rust", "0.35.0"),
}

SDK_INTEGRATIONS = {
    "javascript": ["BrowserTracing", "Replay", "HttpClient", "CaptureConsole", "Dedupe", "GlobalHandlers", "LinkedErrors"],
    "python":     ["DjangoIntegration", "CeleryIntegration", "RedisIntegration", "SqlalchemyIntegration", "LoggingIntegration"],
    "node":       ["Http", "Express", "Mongo", "Postgres", "GraphQL", "Undici"],
    "ruby":       ["Rack", "Rails", "Sidekiq", "Redis", "Puma"],
    "java":       ["SpringMvcIntegration", "SpringBootIntegration", "LogbackIntegration", "JdbcIntegration"],
    "go":         ["echo", "gin", "http", "sql", "grpc", "logrus"],
    "php":        ["LaravelIntegration", "SymfonyIntegration", "GuzzleIntegration"],
    "swift":      ["UIKit", "CoreData", "URLSession", "SwiftUI"],
    "rust":       ["tracing", "tower", "log", "panic"],
}

SDK_PACKAGES = {
    "javascript": [{"name": "npm:@sentry/browser", "version": "8.42.0"}, {"name": "npm:@sentry/tracing", "version": "8.42.0"}],
    "python":     [{"name": "pypi:sentry-sdk", "version": "2.19.2"}],
    "node":       [{"name": "npm:@sentry/node", "version": "8.42.0"}, {"name": "npm:@sentry/profiling-node", "version": "8.42.0"}],
    "ruby":       [{"name": "gem:sentry-ruby", "version": "5.22.1"}, {"name": "gem:sentry-rails", "version": "5.22.1"}],
    "java":       [{"name": "maven:io.sentry:sentry-spring-boot-starter", "version": "7.20.0"}],
    "go":         [{"name": "go:github.com/getsentry/sentry-go", "version": "0.31.1"}],
    "php":        [{"name": "composer:sentry/sentry-laravel", "version": "4.10.0"}],
    "swift":      [{"name": "cocoapods:Sentry", "version": "8.44.0"}],
    "rust":       [{"name": "cargo:sentry", "version": "0.35.0"}],
}

# ---------------------------------------------------------------------------
# Loggers per platform
# ---------------------------------------------------------------------------

LOGGERS_BY_PLATFORM = {
    "javascript": ["console", "sentry.browser", "app.logger", "analytics"],
    "python":     ["django.request", "celery.worker", "app.views", "app.tasks", "gunicorn.error", "sqlalchemy.engine"],
    "node":       ["express", "app.server", "worker", "socket.io", "bull"],
    "ruby":       ["Rails", "Sidekiq", "ActiveRecord", "ActionController"],
    "java":       ["com.example.service", "org.springframework.web", "com.zaxxer.hikari", "org.hibernate"],
    "go":         ["main", "server", "handler", "middleware"],
    "php":        ["laravel", "Illuminate.Http", "App.Services", "queue"],
    "swift":      ["UIKit", "MyApp.ViewModel", "MyApp.Network", "CoreData"],
    "rust":       ["server", "handler", "db", "tracing"],
}

# ---------------------------------------------------------------------------
# Modules (dependency versions) per platform
# ---------------------------------------------------------------------------

MODULES_BY_PLATFORM = {
    "python":     {"django": "4.2.9", "celery": "5.3.6", "redis": "5.0.1", "sqlalchemy": "2.0.25", "requests": "2.31.0", "gunicorn": "21.2.0"},
    "node":       {"express": "4.18.2", "pg": "8.11.3", "redis": "4.6.12", "mongoose": "8.1.0", "jsonwebtoken": "9.0.2"},
    "ruby":       {"rails": "7.1.3", "sidekiq": "7.2.1", "pg": "1.5.4", "redis": "5.1.0", "puma": "6.4.2"},
    "java":       {"spring-boot": "3.2.2", "hibernate": "6.4.2", "hikari": "5.1.0", "jackson": "2.16.1", "kafka-clients": "3.6.1"},
    "go":         {"gin": "1.9.1", "gorm": "1.25.6", "redis": "9.4.0", "grpc": "1.61.0"},
    "php":        {"laravel/framework": "10.43.0", "guzzlehttp/guzzle": "7.8.1", "doctrine/dbal": "3.8.1"},
    "rust":       {"tokio": "1.35.1", "axum": "0.7.4", "sqlx": "0.7.3", "serde": "1.0.195", "reqwest": "0.11.23"},
}

# ---------------------------------------------------------------------------
# Releases, environments, server names, levels
# ---------------------------------------------------------------------------

RELEASES = [
    "1.0.0", "1.0.1", "1.1.0", "1.2.0-beta.1", "2.0.0-rc.1",
    "2.0.0", "2.1.0", "2.1.1", "3.0.0-alpha", "3.0.0",
]

ENVIRONMENTS = ["production", "staging", "development", "canary"]

SERVER_NAMES = [
    "web-01.us-east", "web-02.us-east", "web-03.eu-west",
    "api-01.us-east", "api-02.eu-west", "worker-01.us-east",
    "worker-02.eu-west", "cron-01.us-east",
    None, None,
]

LEVELS = ["fatal", "error", "error", "error", "warning", "warning", "info", "debug"]

# ---------------------------------------------------------------------------
# User / geo data
# ---------------------------------------------------------------------------

USERNAMES = [
    "jdoe", "jsmith", "mgarcia", "ahassan", "ytanaka", "psharma",
    "amueller", "srossi", "wchen", "opetrova", "clopez", "ewilson",
    "bnguyen", "dkim", "fjohnson", "gbrown", "hlee", "imartinez",
]

GEO_DATA = [
    {"country_code": "US", "city": "San Francisco", "region": "CA"},
    {"country_code": "US", "city": "New York", "region": "NY"},
    {"country_code": "US", "city": "Austin", "region": "TX"},
    {"country_code": "DE", "city": "Berlin", "region": "BE"},
    {"country_code": "GB", "city": "London", "region": "ENG"},
    {"country_code": "JP", "city": "Tokyo", "region": "13"},
    {"country_code": "BR", "city": "Sao Paulo", "region": "SP"},
    {"country_code": "IN", "city": "Mumbai", "region": "MH"},
    {"country_code": "FR", "city": "Paris", "region": "IDF"},
    {"country_code": "AU", "city": "Sydney", "region": "NSW"},
    {"country_code": "CA", "city": "Toronto", "region": "ON"},
    {"country_code": "SG", "city": "Singapore", "region": "SG"},
]

EXTRA_KEYS = [
    ("order_id", lambda: random.randint(10000, 99999)),
    ("retry_count", lambda: random.randint(0, 5)),
    ("queue_depth", lambda: random.randint(0, 1000)),
    ("cache_hit", lambda: random.choice([True, False])),
    ("request_size_bytes", lambda: random.randint(100, 50000)),
    ("customer_id", lambda: f"cust_{random.randint(1000, 9999)}"),
    ("feature_flag", lambda: random.choice(["new-checkout", "dark-mode", "beta-search", "v2-api"])),
    ("shard", lambda: random.choice(["us-east-1", "eu-west-1", "ap-south-1"])),
]

# ---------------------------------------------------------------------------
# Error catalog: per-platform error patterns
# ---------------------------------------------------------------------------

JS_ERRORS = [
    ("TypeError", "Cannot read properties of undefined (reading 'map')",
     [("node_modules/react-dom/cjs/react-dom.development.js", "renderWithHooks", None),
      ("src/components/UserList.tsx", "UserList", None),
      ("src/pages/Dashboard.tsx", "Dashboard", None)],
     "onerror", False),
    ("Error", "Hydration failed because the initial UI does not match what was rendered on the server",
     [("node_modules/react-dom/cjs/react-dom.development.js", "hydrateRoot", None),
      ("src/pages/_app.tsx", "App", None),
      (".next/server/pages/_document.js", "renderDocument", None)],
     "onerror", False),
    ("ChunkLoadError", "Loading chunk 7 failed. (missing: /static/js/7.abc123.chunk.js)",
     [("webpack/runtime/ensure chunk.js", "__webpack_require__.e", None),
      ("src/routes/LazyCheckout.tsx", "loadComponent", None),
      ("src/components/App.tsx", "Router", None)],
     "onunhandledrejection", False),
    ("Error", "Minified React error #425; visit https://reactjs.org/docs/error-decoder.html?invariant=425",
     [("node_modules/react-dom/cjs/react-dom.production.min.js", "ue", None),
      ("node_modules/react-dom/cjs/react-dom.production.min.js", "Se", None),
      ("src/components/StreamProvider.tsx", "StreamProvider", None)],
     "onerror", False),
    ("RangeError", "Maximum call stack size exceeded",
     [("src/utils/deepClone.ts", "deepClone", None),
      ("src/utils/deepClone.ts", "deepClone", None),
      ("src/hooks/useFormState.ts", "useFormState", None)],
     "onerror", False),
    ("SyntaxError", "Unexpected token '<' in JSON at position 0",
     [("<anonymous>", "JSON.parse", None),
      ("src/api/client.ts", "fetchJSON", None),
      ("src/hooks/useQuery.ts", "executeQuery", None)],
     "onerror", False),
    ("DOMException", "Failed to execute 'removeChild' on 'Node': The node to be removed is not a child of this node.",
     [("node_modules/react-dom/cjs/react-dom.development.js", "removeChild", None),
      ("src/components/Modal.tsx", "Modal", None),
      ("src/pages/Settings.tsx", "Settings", None)],
     "onerror", False),
    ("TypeError", "Failed to fetch",
     [("src/api/client.ts", "fetchWithRetry", None),
      ("src/api/orders.ts", "submitOrder", None),
      ("src/pages/Checkout.tsx", "handleSubmit", None)],
     "onunhandledrejection", False),
    ("Error", "NEXT_NOT_FOUND",
     [("node_modules/next/dist/client/components/not-found.js", "notFound", None),
      ("src/app/[slug]/page.tsx", "Page", None),
      ("node_modules/next/dist/server/app-render.js", "renderToHTML", None)],
     "onerror", True),
    ("SecurityError", "Blocked a frame with origin \"https://app.example.com\" from accessing a cross-origin frame.",
     [("src/components/IframeEmbed.tsx", "postMessageToChild", None),
      ("src/hooks/useEmbed.ts", "syncState", None),
      ("src/pages/Integrations.tsx", "Integrations", None)],
     "onerror", False),
    ("Error", "Invariant: attempted to hard navigate to the same URL /dashboard",
     [("node_modules/next/dist/client/components/app-router.js", "navigate", None),
      ("src/components/Sidebar.tsx", "handleNavClick", None),
      ("src/layouts/MainLayout.tsx", "MainLayout", None)],
     "onerror", False),
    ("TypeError", "Cannot read properties of null (reading 'addEventListener')",
     [("src/components/VideoPlayer.tsx", "initPlayer", None),
      ("src/hooks/useMediaControls.ts", "setupControls", None),
      ("src/pages/MediaDetail.tsx", "MediaDetail", None)],
     "onerror", False),
    ("ReferenceError", "regeneratorRuntime is not defined",
     [("src/utils/asyncHelpers.js", "asyncFetchData", None),
      ("src/api/legacy.js", "getLegacyUsers", None),
      ("src/pages/Admin.tsx", "Admin", None)],
     "onerror", False),
    ("AbortError", "The user aborted a request.",
     [("src/api/client.ts", "fetchWithTimeout", None),
      ("src/hooks/useSearch.ts", "debouncedSearch", None),
      ("src/components/SearchBar.tsx", "SearchBar", None)],
     "onunhandledrejection", True),
]

PYTHON_ERRORS = [
    ("django.db.utils.IntegrityError", 'duplicate key value violates unique constraint "users_email_key"',
     [("django/db/backends/base/base.py", "_commit", "django.db.backends.base.base"),
      ("app/services/user_service.py", "create_user", "app.services.user_service"),
      ("app/views/registration.py", "register", "app.views.registration")],
     "generic", False),
    ("django.core.exceptions.PermissionDenied", "You do not have permission to perform this action.",
     [("django/contrib/auth/decorators.py", "check_perms", "django.contrib.auth.decorators"),
      ("app/views/admin.py", "delete_user", "app.views.admin"),
      ("app/middleware/audit.py", "process_view", "app.middleware.audit")],
     "generic", False),
    ("celery.exceptions.MaxRetriesExceededError", "Can't retry app.tasks.send_welcome_email[abc-123-def] args:() kwargs:{'user_id': 42}",
     [("celery/app/task.py", "retry", "celery.app.task"),
      ("app/tasks/email.py", "send_welcome_email", "app.tasks.email"),
      ("celery/app/trace.py", "trace_task", "celery.app.trace")],
     "generic", False),
    ("sqlalchemy.exc.OperationalError", '(psycopg2.OperationalError) connection to server at "db-primary.internal" (10.0.1.5), port 5432 failed: Connection refused',
     [("sqlalchemy/engine/base.py", "execute", "sqlalchemy.engine.base"),
      ("app/repositories/order_repo.py", "get_pending_orders", "app.repositories.order_repo"),
      ("app/services/order_service.py", "process_batch", "app.services.order_service")],
     "generic", False),
    ("ValueError", "invalid literal for int() with base 10: 'abc'",
     [("app/serializers/product.py", "validate_quantity", "app.serializers.product"),
      ("app/views/cart.py", "add_to_cart", "app.views.cart"),
      ("django/core/handlers/base.py", "get_response", "django.core.handlers.base")],
     "generic", False),
    ("KeyError", "'user_id'",
     [("app/utils/context.py", "get_current_user", "app.utils.context"),
      ("app/middleware/auth.py", "process_request", "app.middleware.auth"),
      ("django/core/handlers/base.py", "get_response", "django.core.handlers.base")],
     "generic", False),
    ("AttributeError", "'NoneType' object has no attribute 'get'",
     [("app/services/payment.py", "extract_card_info", "app.services.payment"),
      ("app/views/checkout.py", "process_payment", "app.views.checkout"),
      ("django/core/handlers/base.py", "get_response", "django.core.handlers.base")],
     "generic", False),
    ("json.decoder.JSONDecodeError", "Expecting value: line 1 column 1 (char 0)",
     [("json/decoder.py", "raw_decode", "json.decoder"),
      ("app/clients/external_api.py", "parse_response", "app.clients.external_api"),
      ("app/services/sync_service.py", "sync_inventory", "app.services.sync_service")],
     "generic", False),
    ("FileNotFoundError", "[Errno 2] No such file or directory: '/etc/app/config.yaml'",
     [("builtins", "open", None),
      ("app/config/loader.py", "load_config", "app.config.loader"),
      ("app/main.py", "initialize", "app.main")],
     "generic", False),
    ("RuntimeError", "Event loop is closed",
     [("asyncio/base_events.py", "_check_closed", "asyncio.base_events"),
      ("app/workers/async_worker.py", "process_queue", "app.workers.async_worker"),
      ("app/main.py", "shutdown", "app.main")],
     "generic", False),
    ("MemoryError", "Unable to allocate 2.00 GiB for an array with shape (268435456,) and data type float64",
     [("numpy/core/numeric.py", "zeros", "numpy.core.numeric"),
      ("app/analytics/aggregator.py", "build_matrix", "app.analytics.aggregator"),
      ("app/views/reports.py", "generate_report", "app.views.reports")],
     "generic", False),
    ("requests.exceptions.ConnectionError", "HTTPConnectionPool(host='payments.internal', port=8080): Max retries exceeded with url: /charge (Caused by ConnectionRefusedError)",
     [("requests/adapters.py", "send", "requests.adapters"),
      ("app/clients/payment_client.py", "charge", "app.clients.payment_client"),
      ("app/services/billing.py", "bill_customer", "app.services.billing")],
     "generic", False),
    ("django.template.exceptions.TemplateSyntaxError", "Invalid block tag on line 42: 'endblock', expected 'endif'. Did you forget to register or load this tag?",
     [("django/template/base.py", "compile_nodelist", "django.template.base"),
      ("app/views/dashboard.py", "render_dashboard", "app.views.dashboard"),
      ("django/core/handlers/base.py", "get_response", "django.core.handlers.base")],
     "generic", False),
    ("PermissionError", "[Errno 13] Permission denied: '/var/log/app/audit.log'",
     [("builtins", "open", None),
      ("app/logging/file_handler.py", "rotate_log", "app.logging.file_handler"),
      ("app/main.py", "configure_logging", "app.main")],
     "generic", False),
]

NODE_ERRORS = [
    ("Error", "connect ECONNREFUSED 127.0.0.1:5432",
     [("net.js", "TCPConnectWrap.afterConnect", None),
      ("node_modules/pg/lib/connection.js", "Connection.connect", None),
      ("src/db/pool.ts", "getConnection", None),
      ("src/routes/users.ts", "getUsers", None)],
     "generic", False),
    ("TypeError", "Cannot read properties of null (reading 'user')",
     [("src/middleware/auth.ts", "requireAuth", None),
      ("src/routes/profile.ts", "getProfile", None),
      ("node_modules/express/lib/router/layer.js", "Layer.handle", None)],
     "generic", False),
    ("Error", "ENOMEM: not enough memory, read",
     [("fs.js", "FSReqCallback.readFileAfterOpen", None),
      ("src/services/file-processor.ts", "processUpload", None),
      ("src/routes/upload.ts", "handleUpload", None)],
     "generic", False),
    ("MongoServerError", "E11000 duplicate key error collection: mydb.users index: email_1 dup key: { email: \"jane@example.com\" }",
     [("node_modules/mongodb/lib/operations/insert.js", "InsertOneOperation.execute", None),
      ("src/repositories/user-repo.ts", "createUser", None),
      ("src/services/registration.ts", "registerUser", None)],
     "generic", False),
    ("JsonWebTokenError", "jwt malformed",
     [("node_modules/jsonwebtoken/verify.js", "verify", None),
      ("src/middleware/jwt.ts", "verifyToken", None),
      ("src/routes/api.ts", "apiRouter", None)],
     "generic", False),
    ("Error", "ENOENT: no such file or directory, open '/tmp/uploads/abc123.pdf'",
     [("fs.js", "FSReqCallback.oncomplete", None),
      ("src/services/document-service.ts", "getDocument", None),
      ("src/routes/documents.ts", "downloadDocument", None)],
     "generic", False),
    ("SyntaxError", "Unexpected end of JSON input",
     [("<anonymous>", "JSON.parse", None),
      ("src/middleware/body-parser.ts", "parseBody", None),
      ("src/routes/webhooks.ts", "handleWebhook", None)],
     "generic", False),
    ("RangeError", "Invalid time value",
     [("src/utils/date-formatter.ts", "formatDate", None),
      ("src/services/report-builder.ts", "buildReport", None),
      ("src/routes/reports.ts", "generateReport", None)],
     "generic", False),
    ("Error", "querySrv ENOTFOUND _mongodb._tcp.cluster0.mongodb.net",
     [("dns.js", "querySrv", None),
      ("node_modules/mongodb/lib/connection_string.js", "resolveSRVRecord", None),
      ("src/db/mongo.ts", "connect", None)],
     "generic", False),
    ("TimeoutError", "ResourceRequest timed out",
     [("node_modules/generic-pool/lib/Pool.js", "Pool._dispense", None),
      ("src/db/pool.ts", "acquire", None),
      ("src/routes/analytics.ts", "runQuery", None)],
     "generic", False),
    ("Error", "Redis connection to 10.0.2.15:6379 failed - connect ECONNREFUSED",
     [("node_modules/redis/lib/client.js", "RedisClient.connect", None),
      ("src/cache/redis.ts", "getClient", None),
      ("src/services/session-store.ts", "getSession", None)],
     "generic", False),
    ("TypeError", "Converting circular structure to JSON",
     [("<anonymous>", "JSON.stringify", None),
      ("src/middleware/logger.ts", "logRequest", None),
      ("node_modules/express/lib/router/layer.js", "Layer.handle", None)],
     "generic", False),
]

RUBY_ERRORS = [
    ("ActiveRecord::RecordNotFound", "Couldn't find User with 'id'=999",
     [("activerecord/lib/active_record/core.rb", "find", "ActiveRecord::Core"),
      ("app/controllers/users_controller.rb", "show", "UsersController"),
      ("actionpack/lib/action_controller/metal/basic_implicit_render.rb", "send_action", "ActionController")],
     "generic", False),
    ("ActionController::RoutingError", 'No route matches [GET] "/admin/secrets"',
     [("actionpack/lib/action_dispatch/middleware/debug_exceptions.rb", "call", "ActionDispatch::DebugExceptions"),
      ("actionpack/lib/action_dispatch/routing/route_set.rb", "call", "ActionDispatch::Routing::RouteSet"),
      ("config/routes.rb", "routes", None)],
     "generic", False),
    ("ActiveRecord::StatementInvalid", 'PG::UndefinedTable: ERROR:  relation "legacy_users" does not exist',
     [("activerecord/lib/active_record/connection_adapters/postgresql_adapter.rb", "exec_query", "ActiveRecord::ConnectionAdapters::PostgreSQLAdapter"),
      ("app/models/legacy_user.rb", "migrate_data", "LegacyUser"),
      ("app/services/data_migration_service.rb", "run", "DataMigrationService")],
     "generic", False),
    ("Redis::CannotConnectError", "Error connecting to Redis on 127.0.0.1:6379 (Errno::ECONNREFUSED)",
     [("redis/lib/redis/client.rb", "connect", "Redis::Client"),
      ("app/services/cache_service.rb", "fetch", "CacheService"),
      ("app/controllers/products_controller.rb", "index", "ProductsController")],
     "generic", False),
    ("Sidekiq::Shutdown", "job killed by Sidekiq during deploy",
     [("sidekiq/lib/sidekiq/processor.rb", "process", "Sidekiq::Processor"),
      ("app/workers/report_worker.rb", "perform", "ReportWorker"),
      ("sidekiq/lib/sidekiq/launcher.rb", "run", "Sidekiq::Launcher")],
     "generic", False),
    ("ActionView::Template::Error", "undefined method 'full_name' for nil:NilClass",
     [("actionview/lib/action_view/template.rb", "handle_render_error", "ActionView::Template"),
      ("app/views/users/show.html.erb", "_render_template", None),
      ("app/controllers/users_controller.rb", "show", "UsersController")],
     "generic", False),
    ("JWT::DecodeError", "Signature verification failed",
     [("jwt/lib/jwt/decode.rb", "verify_signature", "JWT::Decode"),
      ("app/middleware/jwt_auth.rb", "call", "JwtAuth"),
      ("app/controllers/api/base_controller.rb", "authenticate!", "Api::BaseController")],
     "generic", False),
    ("Rack::Timeout::RequestTimeoutException", "Request ran for longer than 30000ms",
     [("rack-timeout/lib/rack/timeout/core.rb", "call", "Rack::Timeout"),
      ("app/controllers/reports_controller.rb", "generate", "ReportsController"),
      ("actionpack/lib/action_controller/metal.rb", "dispatch", "ActionController::Metal")],
     "generic", False),
    ("NoMethodError", "undefined method 'each' for nil:NilClass",
     [("app/services/notification_service.rb", "send_batch", "NotificationService"),
      ("app/controllers/notifications_controller.rb", "create", "NotificationsController"),
      ("actionpack/lib/action_controller/metal.rb", "dispatch", "ActionController::Metal")],
     "generic", False),
    ("ArgumentError", "wrong number of arguments (given 3, expected 2)",
     [("app/models/order.rb", "calculate_total", "Order"),
      ("app/services/checkout_service.rb", "finalize", "CheckoutService"),
      ("app/controllers/orders_controller.rb", "create", "OrdersController")],
     "generic", False),
    ("Errno::ENOSPC", "No space left on device @ rb_sysopen - /tmp/export_20240115.csv",
     [("app/services/export_service.rb", "write_csv", "ExportService"),
      ("app/workers/export_worker.rb", "perform", "ExportWorker"),
      ("sidekiq/lib/sidekiq/processor.rb", "process", "Sidekiq::Processor")],
     "generic", False),
]

JAVA_ERRORS = [
    ("java.lang.NullPointerException", "Cannot invoke \"String.length()\" because \"str\" is null",
     [("com.example.service.UserService", "validateInput", "com.example.service"),
      ("com.example.controller.UserController", "createUser", "com.example.controller"),
      ("org.springframework.web.servlet.FrameworkServlet", "service", "org.springframework.web.servlet")],
     "generic", False),
    ("org.springframework.web.client.HttpServerErrorException$ServiceUnavailable", "503 Service Unavailable: \"upstream connect error\"",
     [("org.springframework.web.client.DefaultResponseErrorHandler", "handleError", "org.springframework.web.client"),
      ("com.example.client.PaymentClient", "charge", "com.example.client"),
      ("com.example.service.OrderService", "processPayment", "com.example.service")],
     "generic", False),
    ("java.sql.SQLTransientConnectionException", "HikariPool-1 - Connection is not available, request timed out after 30000ms",
     [("com.zaxxer.hikari.pool.HikariPool", "getConnection", "com.zaxxer.hikari.pool"),
      ("com.example.repository.OrderRepository", "findPendingOrders", "com.example.repository"),
      ("com.example.service.OrderService", "processBatch", "com.example.service")],
     "generic", False),
    ("org.hibernate.LazyInitializationException", "could not initialize proxy [com.example.model.UserProfile#42] - no Session",
     [("org.hibernate.proxy.AbstractLazyInitializer", "initialize", "org.hibernate.proxy"),
      ("com.example.service.UserService", "getUserProfile", "com.example.service"),
      ("com.example.controller.UserController", "getProfile", "com.example.controller")],
     "generic", False),
    ("java.util.ConcurrentModificationException", None,
     [("java.util.ArrayList$Itr", "checkForComodification", "java.util"),
      ("com.example.service.CacheManager", "evictExpired", "com.example.service"),
      ("com.example.scheduler.CacheCleanup", "run", "com.example.scheduler")],
     "generic", False),
    ("com.fasterxml.jackson.databind.exc.UnrecognizedPropertyException",
     'Unrecognized field "user_name" (class com.example.dto.UserDTO), not marked as ignorable',
     [("com.fasterxml.jackson.databind.deser.DefaultDeserializationContext", "handleUnknownProperty", "com.fasterxml.jackson.databind.deser"),
      ("com.example.controller.UserController", "updateUser", "com.example.controller"),
      ("org.springframework.web.servlet.FrameworkServlet", "service", "org.springframework.web.servlet")],
     "generic", False),
    ("org.springframework.security.access.AccessDeniedException", "Access is denied",
     [("org.springframework.security.access.vote.AffirmativeBased", "decide", "org.springframework.security.access.vote"),
      ("com.example.controller.AdminController", "deleteUser", "com.example.controller"),
      ("org.springframework.web.servlet.FrameworkServlet", "service", "org.springframework.web.servlet")],
     "generic", False),
    ("java.lang.OutOfMemoryError", "Java heap space",
     [("com.example.service.ReportService", "generateLargeReport", "com.example.service"),
      ("com.example.controller.ReportController", "export", "com.example.controller"),
      ("org.springframework.web.servlet.FrameworkServlet", "service", "org.springframework.web.servlet")],
     "generic", False),
    ("javax.validation.ConstraintViolationException", "Validation failed for classes [com.example.model.Order] during persist",
     [("org.hibernate.cfg.beanvalidation.BeanValidationEventListener", "validate", "org.hibernate.cfg.beanvalidation"),
      ("com.example.repository.OrderRepository", "save", "com.example.repository"),
      ("com.example.service.OrderService", "createOrder", "com.example.service")],
     "generic", False),
    ("io.netty.handler.timeout.ReadTimeoutException", None,
     [("io.netty.handler.timeout.ReadTimeoutHandler", "readTimedOut", "io.netty.handler.timeout"),
      ("com.example.client.DownstreamClient", "fetchData", "com.example.client"),
      ("com.example.service.AggregatorService", "aggregate", "com.example.service")],
     "generic", False),
    ("java.lang.IllegalStateException", "Failed to load ApplicationContext for [WebMergedContextConfiguration]",
     [("org.springframework.test.context.cache.DefaultCacheAwareContextLoaderDelegate", "loadContext", "org.springframework.test.context.cache"),
      ("com.example.ApplicationTests", "contextLoads", "com.example"),
      ("org.junit.jupiter.engine.execution.ExecutableInvoker", "invoke", "org.junit.jupiter.engine.execution")],
     "generic", False),
    ("org.apache.kafka.common.errors.TimeoutException", "Topic my-topic not present in metadata after 60000 ms",
     [("org.apache.kafka.clients.producer.KafkaProducer", "doSend", "org.apache.kafka.clients.producer"),
      ("com.example.messaging.EventPublisher", "publish", "com.example.messaging"),
      ("com.example.service.OrderService", "placeOrder", "com.example.service")],
     "generic", False),
]

GO_ERRORS = [
    ("runtime error", "index out of range [5] with length 3",
     [("runtime/panic.go", "goPanicIndex", "runtime"),
      ("internal/handlers/user.go", "GetUser", "internal/handlers"),
      ("internal/router/router.go", "ServeHTTP", "internal/router")],
     "generic", False),
    ("context.DeadlineExceeded", "context deadline exceeded",
     [("context/context.go", "WithDeadline.func1", "context"),
      ("internal/clients/payment.go", "Charge", "internal/clients"),
      ("internal/handlers/checkout.go", "HandleCheckout", "internal/handlers")],
     "generic", False),
    ("net.OpError", "dial tcp 10.0.0.5:5432: connect: connection refused",
     [("net/dial.go", "(*Dialer).DialContext", "net"),
      ("database/sql/sql.go", "(*DB).conn", "database/sql"),
      ("internal/repository/user.go", "FindByID", "internal/repository")],
     "generic", False),
    ("json.UnmarshalTypeError", 'json: cannot unmarshal string into Go value of type int',
     [("encoding/json/decode.go", "(*decodeState).unmarshal", "encoding/json"),
      ("internal/handlers/api.go", "parseRequest", "internal/handlers"),
      ("internal/router/router.go", "ServeHTTP", "internal/router")],
     "generic", False),
    ("net/http.badStringError", "http: server gave HTTP response to HTTPS client",
     [("net/http/transport.go", "(*Transport).roundTrip", "net/http"),
      ("internal/clients/upstream.go", "Call", "internal/clients"),
      ("internal/handlers/proxy.go", "HandleProxy", "internal/handlers")],
     "generic", False),
    ("sql.ErrConnDone", "sql: database is closed",
     [("database/sql/sql.go", "(*DB).exec", "database/sql"),
      ("internal/repository/config.go", "Update", "internal/repository"),
      ("internal/handlers/config.go", "HandleUpdateConfig", "internal/handlers")],
     "generic", False),
    ("runtime error", "assignment to entry in nil map",
     [("runtime/panic.go", "goPanicNilMapAssign", "runtime"),
      ("internal/cache/local.go", "Set", "internal/cache"),
      ("internal/handlers/warmup.go", "WarmCache", "internal/handlers")],
     "generic", False),
    ("x509.CertificateInvalidError", "tls: failed to verify certificate: x509: certificate has expired or is not yet valid",
     [("crypto/tls/handshake_client.go", "(*clientHandshakeState).verifyServerCertificate", "crypto/tls"),
      ("internal/clients/mtls.go", "Connect", "internal/clients"),
      ("internal/handlers/federation.go", "HandleSync", "internal/handlers")],
     "generic", False),
    ("io.ErrClosedPipe", "io: read/write on closed pipe",
     [("io/pipe.go", "(*pipe).write", "io"),
      ("internal/streaming/writer.go", "Write", "internal/streaming"),
      ("internal/handlers/stream.go", "HandleStream", "internal/handlers")],
     "generic", False),
    ("redis.Error", "WRONGTYPE Operation against a key holding the wrong kind of value",
     [("github.com/go-redis/redis/v9/command.go", "(*StatusCmd).Result", "github.com/go-redis/redis/v9"),
      ("internal/cache/redis.go", "IncrementCounter", "internal/cache"),
      ("internal/handlers/rate_limit.go", "CheckRate", "internal/handlers")],
     "generic", False),
    ("grpc.StatusError", "rpc error: code = Unavailable desc = connection closed before server preface received",
     [("google.golang.org/grpc/transport/http2_client.go", "(*http2Client).Close", "google.golang.org/grpc/transport"),
      ("internal/clients/grpc_client.go", "Call", "internal/clients"),
      ("internal/handlers/rpc.go", "HandleRPC", "internal/handlers")],
     "generic", False),
]

PHP_ERRORS = [
    ("Illuminate\\Database\\QueryException",
     "SQLSTATE[23000]: Integrity constraint violation: 1062 Duplicate entry 'jane@example.com' for key 'users_email_unique'",
     [("vendor/laravel/framework/src/Illuminate/Database/Connection.php", "run", "Illuminate\\Database"),
      ("app/Services/UserService.php", "register", "App\\Services"),
      ("app/Http/Controllers/Auth/RegisterController.php", "store", "App\\Http\\Controllers\\Auth")],
     "generic", False),
    ("Symfony\\Component\\HttpKernel\\Exception\\NotFoundHttpException",
     'The route "api/v2/users" could not be found.',
     [("vendor/laravel/framework/src/Illuminate/Routing/RouteCollection.php", "match", "Illuminate\\Routing"),
      ("vendor/laravel/framework/src/Illuminate/Routing/Router.php", "findRoute", "Illuminate\\Routing"),
      ("app/Http/Middleware/ApiVersion.php", "handle", "App\\Http\\Middleware")],
     "generic", False),
    ("ErrorException", "Trying to access array offset on value of type null",
     [("app/Services/CartService.php", "getItemPrice", "App\\Services"),
      ("app/Http/Controllers/CartController.php", "update", "App\\Http\\Controllers"),
      ("vendor/laravel/framework/src/Illuminate/Routing/Controller.php", "callAction", "Illuminate\\Routing")],
     "generic", False),
    ("Illuminate\\Auth\\AuthenticationException", "Unauthenticated.",
     [("vendor/laravel/framework/src/Illuminate/Auth/Middleware/Authenticate.php", "handle", "Illuminate\\Auth\\Middleware"),
      ("app/Http/Controllers/Api/OrderController.php", "index", "App\\Http\\Controllers\\Api"),
      ("vendor/laravel/framework/src/Illuminate/Pipeline/Pipeline.php", "then", "Illuminate\\Pipeline")],
     "generic", True),
    ("GuzzleHttp\\Exception\\ConnectException",
     "cURL error 28: Operation timed out after 30001 milliseconds with 0 bytes received",
     [("vendor/guzzlehttp/guzzle/src/Handler/CurlHandler.php", "__invoke", "GuzzleHttp\\Handler"),
      ("app/Services/PaymentGateway.php", "charge", "App\\Services"),
      ("app/Http/Controllers/CheckoutController.php", "processPayment", "App\\Http\\Controllers")],
     "generic", False),
    ("League\\Flysystem\\UnableToWriteFile",
     "Unable to write file at location: uploads/avatars/user_42.jpg. Disk full.",
     [("vendor/league/flysystem/src/Local/LocalFilesystemAdapter.php", "write", "League\\Flysystem\\Local"),
      ("app/Services/AvatarService.php", "upload", "App\\Services"),
      ("app/Http/Controllers/ProfileController.php", "updateAvatar", "App\\Http\\Controllers")],
     "generic", False),
    ("PDOException", "SQLSTATE[HY000] [2002] Connection refused",
     [("vendor/laravel/framework/src/Illuminate/Database/Connectors/Connector.php", "createPdoConnection", "Illuminate\\Database\\Connectors"),
      ("app/Providers/AppServiceProvider.php", "boot", "App\\Providers"),
      ("vendor/laravel/framework/src/Illuminate/Foundation/Application.php", "boot", "Illuminate\\Foundation")],
     "generic", False),
    ("BadMethodCallException", "Call to undefined method App\\Models\\User::fullName()",
     [("vendor/laravel/framework/src/Illuminate/Support/Traits/ForwardsCalls.php", "throwBadMethodCallException", "Illuminate\\Support\\Traits"),
      ("app/Http/Controllers/UserController.php", "show", "App\\Http\\Controllers"),
      ("vendor/laravel/framework/src/Illuminate/Routing/Controller.php", "callAction", "Illuminate\\Routing")],
     "generic", False),
    ("Illuminate\\Validation\\ValidationException", "The given data was invalid.",
     [("vendor/laravel/framework/src/Illuminate/Validation/Validator.php", "validate", "Illuminate\\Validation"),
      ("app/Http/Requests/StoreOrderRequest.php", "rules", "App\\Http\\Requests"),
      ("app/Http/Controllers/OrderController.php", "store", "App\\Http\\Controllers")],
     "generic", True),
    ("RuntimeException", "The session is not started.",
     [("vendor/symfony/http-foundation/Session/Session.php", "start", "Symfony\\Component\\HttpFoundation\\Session"),
      ("app/Http/Middleware/SessionTimeout.php", "handle", "App\\Http\\Middleware"),
      ("vendor/laravel/framework/src/Illuminate/Pipeline/Pipeline.php", "then", "Illuminate\\Pipeline")],
     "generic", False),
]

SWIFT_ERRORS = [
    ("EXC_BAD_ACCESS", "Thread 1: EXC_BAD_ACCESS (code=1, address=0x0000000000000010)",
     [("libswiftCore.dylib", "swift_unknownObjectRetain", None),
      ("MyApp/ViewModels/ProfileViewModel.swift", "loadProfile()", None),
      ("MyApp/Views/ProfileView.swift", "body.getter", None)],
     "mach", False),
    ("NSInternalInconsistencyException", "Invalid update: invalid number of rows in section 0. The number of rows after the update (5) must be equal to the number before (3), plus or minus insertions (1) and deletions (0).",
     [("UIKitCore", "-[UITableView _endCellAnimationsWithContext:]", None),
      ("MyApp/Controllers/FeedViewController.swift", "reloadData()", None),
      ("MyApp/Services/FeedService.swift", "fetchFeed(completion:)", None)],
     "nsexception", False),
    ("Fatal error", "Unexpectedly found nil while unwrapping an Optional value",
     [("libswiftCore.dylib", "swift_fatalError", None),
      ("MyApp/Services/AuthService.swift", "currentUser.getter", None),
      ("MyApp/ViewModels/SettingsViewModel.swift", "loadSettings()", None)],
     "generic", False),
    ("NSURLErrorDomain", "A server with the specified hostname could not be found. (kCFErrorDomainCFNetwork:-1003)",
     [("CFNetwork", "CFURLSessionTaskCompleted", None),
      ("MyApp/Networking/APIClient.swift", "request(_:completion:)", None),
      ("MyApp/ViewModels/HomeViewModel.swift", "fetchDashboard()", None)],
     "nsexception", False),
    ("CoreData", "Failed to call designated initializer on NSManagedObject class 'UserEntity'",
     [("CoreData", "-[NSManagedObject initWithEntity:insertIntoManagedObjectContext:]", None),
      ("MyApp/Persistence/CoreDataStack.swift", "createUser(_:)", None),
      ("MyApp/Services/SyncService.swift", "syncUsers()", None)],
     "nsexception", False),
    ("NSRangeException", "*** -[__NSArrayM objectAtIndex:]: index 5 beyond bounds [0 .. 2]",
     [("CoreFoundation", "-[__NSArrayM objectAtIndex:]", None),
      ("MyApp/ViewModels/CartViewModel.swift", "removeItem(at:)", None),
      ("MyApp/Views/CartView.swift", "deleteRow(_:)", None)],
     "nsexception", False),
    ("NSCocoaErrorDomain", "The file couldn't be saved because a file with the same name already exists. (Code=516)",
     [("Foundation", "-[NSFileManager moveItemAtURL:toURL:error:]", None),
      ("MyApp/Services/FileManager.swift", "cacheImage(_:)", None),
      ("MyApp/ViewModels/GalleryViewModel.swift", "downloadImage(url:)", None)],
     "nsexception", False),
    ("Fatal error", "Index out of range",
     [("libswiftCore.dylib", "swift_fatalError", None),
      ("MyApp/Models/Playlist.swift", "subscript(_:).getter", None),
      ("MyApp/ViewModels/PlayerViewModel.swift", "skipToNext()", None)],
     "generic", False),
    ("EXC_BREAKPOINT", "Thread 1: EXC_BREAKPOINT (code=1, subcode=0x1a2b3c4d)",
     [("libswiftCore.dylib", "swift_dynamicCast", None),
      ("MyApp/Services/DeepLinkHandler.swift", "handle(url:)", None),
      ("MyApp/AppDelegate.swift", "application(_:open:options:)", None)],
     "mach", False),
    ("CryptoTokenKit.TKError", "Authentication failed: biometric lockout",
     [("LocalAuthentication", "-[LAContext evaluatePolicy:localizedReason:reply:]", None),
      ("MyApp/Services/BiometricAuth.swift", "authenticate(completion:)", None),
      ("MyApp/Views/LoginView.swift", "loginWithBiometrics()", None)],
     "generic", False),
]

RUST_ERRORS = [
    ("panic", "called `Option::unwrap()` on a `None` value",
     [("core/src/option.rs", "unwrap", "core::option"),
      ("src/handlers/user.rs", "get_user", "crate::handlers::user"),
      ("src/router.rs", "dispatch", "crate::router")],
     "generic", False),
    ("panic", "index out of bounds: the len is 3 but the index is 5",
     [("core/src/panicking.rs", "panic_bounds_check", "core::panicking"),
      ("src/services/batch.rs", "process_chunk", "crate::services::batch"),
      ("src/handlers/import.rs", "handle_import", "crate::handlers::import")],
     "generic", False),
    ("reqwest::Error", "error sending request for url (http://payments.internal:8080/charge): connection refused",
     [("reqwest/src/error.rs", "from", "reqwest::error"),
      ("src/clients/payment.rs", "charge", "crate::clients::payment"),
      ("src/handlers/checkout.rs", "handle_checkout", "crate::handlers::checkout")],
     "generic", False),
    ("serde_json::Error", "missing field `user_id` at line 1 column 42",
     [("serde_json/src/de.rs", "from_str", "serde_json::de"),
      ("src/handlers/api.rs", "parse_request", "crate::handlers::api"),
      ("src/router.rs", "dispatch", "crate::router")],
     "generic", False),
    ("diesel::result::Error", "NotFound",
     [("diesel/src/result.rs", "query_result", "diesel::result"),
      ("src/repositories/user.rs", "find_by_id", "crate::repositories::user"),
      ("src/handlers/user.rs", "get_user", "crate::handlers::user")],
     "generic", False),
    ("tokio::time::error::Elapsed", "deadline has elapsed",
     [("tokio/src/time/timeout.rs", "poll", "tokio::time::timeout"),
      ("src/clients/downstream.rs", "fetch", "crate::clients::downstream"),
      ("src/handlers/proxy.rs", "handle_proxy", "crate::handlers::proxy")],
     "generic", False),
    ("sqlx::Error", "error communicating with database: connection refused",
     [("sqlx-core/src/error.rs", "from", "sqlx_core::error"),
      ("src/db/pool.rs", "acquire", "crate::db::pool"),
      ("src/handlers/health.rs", "check_db", "crate::handlers::health")],
     "generic", False),
    ("std::io::Error", "Permission denied (os error 13)",
     [("std/src/io/error.rs", "last_os_error", "std::io"),
      ("src/storage/disk.rs", "write_file", "crate::storage::disk"),
      ("src/handlers/upload.rs", "handle_upload", "crate::handlers::upload")],
     "generic", False),
    ("hyper::Error", "connection reset by peer",
     [("hyper/src/error.rs", "from", "hyper::error"),
      ("src/server.rs", "accept_connection", "crate::server"),
      ("src/main.rs", "main", "crate")],
     "generic", False),
    ("panic", "called `Result::unwrap()` on an `Err` value: \"config.toml not found\"",
     [("core/src/result.rs", "unwrap", "core::result"),
      ("src/config.rs", "load", "crate::config"),
      ("src/main.rs", "main", "crate")],
     "generic", False),
    ("anyhow::Error", "Failed to parse configuration: expected string, found integer",
     [("toml/src/de.rs", "from_str", "toml::de"),
      ("src/config.rs", "parse", "crate::config"),
      ("src/main.rs", "main", "crate")],
     "generic", False),
]

ERROR_CATALOG = {
    "javascript": JS_ERRORS,
    "python": PYTHON_ERRORS,
    "node": NODE_ERRORS,
    "ruby": RUBY_ERRORS,
    "java": JAVA_ERRORS,
    "go": GO_ERRORS,
    "php": PHP_ERRORS,
    "swift": SWIFT_ERRORS,
    "rust": RUST_ERRORS,
}

# ---------------------------------------------------------------------------
# Transactions per platform
# ---------------------------------------------------------------------------

TRANSACTIONS_BY_PLATFORM = {
    "javascript": [
        "GET /", "GET /dashboard", "GET /settings", "GET /checkout",
        "POST /api/cart", "GET /products/:id", "GET /search",
        "pageload /", "pageload /dashboard", "navigation /checkout",
    ],
    "python": [
        "GET /api/v1/users", "POST /api/v1/users", "GET /api/v1/users/{id}",
        "PUT /api/v1/users/{id}", "DELETE /api/v1/users/{id}",
        "GET /api/v1/orders", "POST /api/v1/orders",
        "POST /api/v1/auth/login", "POST /api/v1/auth/refresh",
        "celery.task.send_email", "celery.task.process_report",
    ],
    "node": [
        "GET /api/v1/notifications", "POST /api/v1/notifications",
        "GET /api/v1/search", "POST /api/v1/webhooks/stripe",
        "POST /api/v1/upload", "GET /api/v1/feed",
        "ws.connect", "ws.message",
    ],
    "ruby": [
        "UsersController#index", "UsersController#show",
        "OrdersController#create", "ProductsController#index",
        "SessionsController#create", "Admin::DashboardController#show",
        "Sidekiq/ReportWorker", "Sidekiq/ImportWorker",
    ],
    "java": [
        "GET /api/v1/payments", "POST /api/v1/payments",
        "GET /api/v1/orders/{id}", "POST /api/v1/orders",
        "GET /actuator/health", "POST /api/v1/refunds",
        "KafkaConsumer.poll", "ScheduledTask.reconcile",
    ],
    "go": [
        "GET /api/v1/config", "PUT /api/v1/config",
        "GET /healthz", "GET /readyz",
        "POST /api/v1/images/resize", "GET /api/v1/status",
        "grpc.unary /service.Check", "grpc.unary /service.Process",
    ],
    "php": [
        "GET /products", "GET /products/{slug}", "POST /cart/add",
        "POST /checkout", "GET /orders/{id}",
        "GET /api/v1/affiliates", "POST /api/v1/campaigns",
        "artisan:queue:work", "artisan:schedule:run",
    ],
    "swift": [
        "app.launch", "app.background_fetch",
        "UIViewController.viewDidLoad", "URLSession.dataTask",
        "CoreData.save", "CoreData.fetch",
        "ui.tap ProfileView", "ui.scroll FeedView",
    ],
    "rust": [
        "GET /api/v1/metrics", "POST /api/v1/ingest",
        "GET /healthz", "POST /api/v1/flush",
        "task.consume_queue", "task.compact_storage",
        "grpc /ingest.Collector.Push", "grpc /health.Health.Check",
    ],
}

# ---------------------------------------------------------------------------
# Messages per platform
# ---------------------------------------------------------------------------

MESSAGES_BY_PLATFORM = {
    "javascript": [
        "User session expired", "Service worker registration failed",
        "WebSocket reconnecting", "Local storage quota exceeded",
        "Third-party script blocked by CSP",
    ],
    "python": [
        "Database connection pool exhausted", "Cache miss rate above threshold",
        "Rate limit exceeded for API key", "Background job retry limit reached",
        "Celery worker heartbeat missed",
    ],
    "node": [
        "Redis connection dropped, reconnecting", "Event loop lag detected: 250ms",
        "Memory usage above 80% threshold", "Upstream service returned 502",
        "WebSocket client disconnected unexpectedly",
    ],
    "ruby": [
        "Sidekiq queue depth exceeds threshold", "ActiveRecord connection pool drained",
        "Action Cable channel subscription failed", "Puma worker timeout, restarting",
    ],
    "java": [
        "HikariPool connection leak detected", "Kafka consumer lag above threshold",
        "GC pause exceeded 500ms", "Circuit breaker opened for payment-service",
        "Thread pool exhausted for async-executor",
    ],
    "go": [
        "Goroutine count exceeds 10000", "gRPC keepalive timeout",
        "TLS certificate expiring in 7 days", "Config reload triggered",
        "Rate limiter bucket depleted",
    ],
    "php": [
        "Opcache full, forcing restart", "Session garbage collection running",
        "Queue worker memory limit reached", "Redis session store unreachable",
    ],
    "swift": [
        "Background fetch completed with no new data",
        "Push notification registration failed",
        "Keychain access denied after device restore",
        "Low memory warning received",
    ],
    "rust": [
        "Connection pool exhausted, queuing requests",
        "Tokio runtime shutdown timeout",
        "Channel send failed: receiver dropped",
        "Disk usage above 90%, enabling backpressure",
    ],
}

# ---------------------------------------------------------------------------
# Context generators per platform
# ---------------------------------------------------------------------------

OS_CHOICES = {
    "javascript": [("Windows", "11"), ("macOS", "14.2"), ("Linux", "6.1"), ("Android", "14"), ("iOS", "17.2")],
    "python":     [("Linux", "6.1.0-rpi"), ("Linux", "5.15.0-generic"), ("macOS", "14.2"), ("Windows", "10")],
    "node":       [("Linux", "6.1.0"), ("Linux", "5.15.0"), ("macOS", "14.2"), ("Alpine Linux", "3.19")],
    "ruby":       [("Linux", "6.1.0"), ("Linux", "5.15.0"), ("macOS", "14.2")],
    "java":       [("Linux", "6.1.0"), ("Linux", "5.15.0"), ("Windows Server", "2022")],
    "go":         [("Linux", "6.1.0"), ("Linux", "5.15.0"), ("Alpine Linux", "3.19")],
    "php":        [("Linux", "6.1.0"), ("Linux", "5.15.0"), ("macOS", "14.2")],
    "swift":      [("iOS", "17.2"), ("iOS", "16.7"), ("iPadOS", "17.2"), ("macOS", "14.2"), ("watchOS", "10.2")],
    "rust":       [("Linux", "6.1.0"), ("Linux", "5.15.0"), ("Alpine Linux", "3.19")],
}

RUNTIME_INFO = {
    "javascript": ("Browser", None),
    "python":     ("CPython", "3.12.1"),
    "node":       ("Node.js", "20.11.0"),
    "ruby":       ("CRuby", "3.3.0"),
    "java":       ("OpenJDK", "21.0.1"),
    "go":         ("Go", "1.22.0"),
    "php":        ("PHP", "8.3.2"),
    "swift":      ("Swift", "5.9.2"),
    "rust":       ("Rust", "1.82.0"),
}

BROWSERS = [
    ("Chrome", "121.0.6167.85"), ("Firefox", "122.0"), ("Safari", "17.2.1"),
    ("Edge", "121.0.2277.83"), ("Opera", "106.0"),
]

IOS_DEVICES = [
    ("iPhone 15 Pro", "arm64"), ("iPhone 15", "arm64"), ("iPhone 14", "arm64"),
    ("iPhone SE (3rd gen)", "arm64"), ("iPad Pro (12.9-inch)", "arm64"),
    ("Apple Watch Series 9", "arm64"),
]

def make_contexts(platform):
    ctx = {}
    os_name, os_version = random.choice(OS_CHOICES[platform])
    ctx["os"] = {"name": os_name, "version": os_version}

    rt_name, rt_version = RUNTIME_INFO[platform]
    if rt_version:
        ctx["runtime"] = {"name": rt_name, "version": rt_version}
    else:
        browser_name, browser_ver = random.choice(BROWSERS)
        ctx["runtime"] = {"name": browser_name, "version": browser_ver}
        ctx["browser"] = {"name": browser_name, "version": browser_ver, "type": "browser"}

    if platform == "swift":
        device = random.choice(IOS_DEVICES)
        ctx["device"] = {
            "name": device[0], "arch": device[1], "family": "iPhone",
            "model": device[0], "simulator": False,
            "battery_level": round(random.uniform(5.0, 100.0), 1),
            "charging": random.choice([True, False]),
        }
        ctx["app"] = {
            "app_name": "MyApp",
            "app_version": random.choice(RELEASES),
            "app_build": str(random.randint(100, 999)),
            "app_identifier": "com.example.myapp",
        }

    return ctx


# ---------------------------------------------------------------------------
# Breadcrumb generators
# ---------------------------------------------------------------------------

def _bc_timestamp(event_ts, index, total):
    """Breadcrumb timestamps in chronological order before the event."""
    offset = (total - index) * random.randint(1, 10)
    return event_ts - offset


def make_breadcrumbs(platform, event_ts):
    count = random.randint(5, 10)
    crumbs = []
    generators = BREADCRUMB_GENERATORS[platform]
    for i in range(count):
        ts = _bc_timestamp(event_ts, i, count)
        gen = random.choice(generators)
        crumb = gen(ts)
        crumbs.append(crumb)
    return {"values": crumbs}


def _bc_navigation(ts):
    pages = ["/", "/dashboard", "/settings", "/checkout", "/login", "/profile", "/search", "/orders"]
    f, t = random.sample(pages, 2)
    return {"timestamp": ts, "type": "navigation", "category": "navigation",
            "level": "info", "data": {"from": f, "to": t}}

def _bc_http(ts):
    urls = ["/api/users", "/api/orders", "/api/products", "/api/auth/me",
            "/api/cart", "/api/search", "/api/notifications"]
    methods = ["GET", "GET", "GET", "POST", "PUT", "DELETE"]
    statuses = [200, 200, 200, 200, 201, 204, 400, 401, 404, 500]
    method = random.choice(methods)
    url = random.choice(urls)
    status = random.choice(statuses)
    level = "error" if status >= 400 else "info"
    return {"timestamp": ts, "type": "http", "category": "xhr",
            "level": level, "message": f"{method} {url}",
            "data": {"url": f"https://api.example.com{url}", "method": method, "status_code": status}}

def _bc_click(ts):
    targets = ["button.submit-btn", "button.login", "a.nav-link", "div.card",
                "button#checkout", "input.search", "button.close-modal"]
    return {"timestamp": ts, "type": "default", "category": "ui.click",
            "level": "info", "message": random.choice(targets)}

def _bc_console(ts):
    msgs = ["Deprecation warning: componentWillMount has been renamed",
            "Warning: Each child in a list should have a unique key prop",
            "API response cached", "Feature flag 'new-checkout' evaluated to true",
            "WebSocket connection established", "Timer started for analytics"]
    levels = ["warning", "warning", "info", "info", "debug", "debug"]
    idx = random.randrange(len(msgs))
    return {"timestamp": ts, "type": "default", "category": "console",
            "level": levels[idx], "message": msgs[idx]}

def _bc_query(ts):
    queries = ["SELECT * FROM users WHERE id = $1", "INSERT INTO orders (...) VALUES (...)",
               "UPDATE products SET stock = stock - 1 WHERE id = $1",
               "SELECT COUNT(*) FROM events WHERE project_id = $1",
               "DELETE FROM sessions WHERE expired_at < NOW()"]
    return {"timestamp": ts, "type": "default", "category": "query",
            "level": "debug", "message": random.choice(queries)}

def _bc_log(ts):
    msgs = ["Starting request processing", "User authenticated successfully",
            "Cache miss for key user:42", "Retrying failed operation (attempt 2/3)",
            "Background job enqueued", "Rate limit check passed"]
    return {"timestamp": ts, "type": "default", "category": "log",
            "level": random.choice(["info", "debug", "warning"]),
            "message": random.choice(msgs)}

def _bc_user(ts):
    actions = ["Logged in", "Changed password", "Updated profile", "Enabled 2FA",
               "Added payment method", "Viewed order history"]
    return {"timestamp": ts, "type": "default", "category": "user",
            "level": "info", "message": random.choice(actions)}

BREADCRUMB_GENERATORS = {
    "javascript": [_bc_navigation, _bc_http, _bc_click, _bc_console, _bc_user],
    "python":     [_bc_http, _bc_query, _bc_log, _bc_user],
    "node":       [_bc_http, _bc_query, _bc_log, _bc_console],
    "ruby":       [_bc_http, _bc_query, _bc_log, _bc_user],
    "java":       [_bc_http, _bc_query, _bc_log, _bc_user],
    "go":         [_bc_http, _bc_query, _bc_log],
    "php":        [_bc_http, _bc_query, _bc_log, _bc_user, _bc_navigation],
    "swift":      [_bc_navigation, _bc_http, _bc_click, _bc_user, _bc_log],
    "rust":       [_bc_http, _bc_query, _bc_log],
}

# ---------------------------------------------------------------------------
# Request context (for web-facing platforms)
# ---------------------------------------------------------------------------

USER_AGENTS = [
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/121.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_2) AppleWebKit/605.1.15 Safari/17.2.1",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148",
    "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/121.0.0.0 Mobile Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64; rv:122.0) Gecko/20100101 Firefox/122.0",
]

def make_request_context(platform, project_name):
    """Generate a realistic request context for web-facing projects."""
    if platform in ("swift",):
        return None

    methods = ["GET", "GET", "GET", "POST", "POST", "PUT", "DELETE"]
    paths = ["/api/v1/users", "/api/v1/orders", "/api/v1/products",
             "/api/v1/auth/login", "/api/v1/cart", "/checkout", "/dashboard",
             "/api/v1/search", "/api/v1/webhooks/stripe", "/api/v1/upload"]
    method = random.choice(methods)
    path = random.choice(paths)
    domain = f"{project_name}.example.com"
    qs_options = ["", "page=1&limit=25", "sort=created_at&order=desc",
                  f"q={random.choice(['shoes', 'laptop', 'phone'])}",
                  f"user_id={random.randint(100, 9999)}"]

    remote_addr = f"{random.randint(1,223)}.{random.randint(0,255)}.{random.randint(0,255)}.{random.randint(1,254)}"

    return {
        "url": f"https://{domain}{path}",
        "method": method,
        "headers": {
            "Content-Type": "application/json",
            "User-Agent": random.choice(USER_AGENTS),
            "X-Request-ID": str(uuid.uuid4()),
            "Accept": "application/json",
        },
        "query_string": random.choice(qs_options),
        "env": {
            "REMOTE_ADDR": remote_addr,
            "SERVER_NAME": domain,
        },
    }


# ---------------------------------------------------------------------------
# Tags
# ---------------------------------------------------------------------------

def make_tags(platform, project, handled, transaction_name=None):
    tags = {
        "handled": "yes" if handled else "no",
        "environment": random.choice(ENVIRONMENTS),
    }

    if platform == "javascript":
        b_name, b_ver = random.choice(BROWSERS)
        tags["browser"] = f"{b_name} {b_ver.split('.')[0]}"
        tags["browser.name"] = b_name
        os_name, os_ver = random.choice(OS_CHOICES["javascript"])
        tags["os"] = f"{os_name} {os_ver}"
        tags["os.name"] = os_name
        url_paths = ["/dashboard", "/checkout", "/settings", "/products", "/orders"]
        tags["url"] = f"https://{project['name']}.example.com{random.choice(url_paths)}"
    elif platform == "swift":
        device = random.choice(IOS_DEVICES)
        tags["device"] = device[0]
        tags["os"] = random.choice(["iOS 17.2", "iOS 16.7", "iPadOS 17.2"])
    else:
        os_name, os_ver = random.choice(OS_CHOICES[platform])
        tags["os"] = f"{os_name} {os_ver}"
        tags["os.name"] = os_name

    if transaction_name:
        tags["transaction"] = transaction_name

    # Web platforms get a url tag
    if platform in ("javascript", "python", "node", "ruby", "java", "php") and "url" not in tags:
        url_paths = ["/dashboard", "/checkout", "/settings", "/api/v1/users", "/api/v1/orders"]
        tags["url"] = f"https://{project['name']}.example.com{random.choice(url_paths)}"

    tiers = ["free", "free", "free", "starter", "starter", "pro", "pro", "enterprise"]
    tags["customer_tier"] = random.choice(tiers)

    return tags


# ---------------------------------------------------------------------------
# CSP reports
# ---------------------------------------------------------------------------

CSP_DIRECTIVES = ["script-src", "style-src", "img-src", "connect-src", "font-src", "frame-src"]
CSP_BLOCKED_URIS = [
    "https://evil.example.com", "inline", "eval",
    "https://cdn.tracking.io/pixel.js", "https://ads.network.com/load",
    "data:", "blob:", "https://unknown-cdn.net/widget.js",
]
CSP_URLS = [
    "https://app.example.com/dashboard", "https://app.example.com/settings",
    "https://app.example.com/checkout", "https://www.example.com/",
]


# ---------------------------------------------------------------------------
# Monitor (check-in) slugs
# ---------------------------------------------------------------------------

MONITOR_SLUGS = [
    ("send-daily-digest", "0 8 * * *"),
    ("cleanup-expired-sessions", "*/15 * * * *"),
    ("sync-inventory", "0 */2 * * *"),
    ("generate-daily-report", "0 6 * * *"),
    ("rotate-api-keys", "0 0 1 * *"),
    ("prune-old-events", "0 3 * * 0"),
    ("refresh-search-index", "*/30 * * * *"),
    ("backup-database", "0 2 * * *"),
    ("check-ssl-certificates", "0 9 * * 1"),
    ("aggregate-metrics", "*/5 * * * *"),
]


# ---------------------------------------------------------------------------
# User feedback comments
# ---------------------------------------------------------------------------

USER_REPORT_COMMENTS = [
    "I was trying to checkout and got a blank page after applying a coupon.",
    "The page froze when I clicked 'Submit Order' and I had to refresh.",
    "I keep getting logged out every few minutes. Very frustrating.",
    "Search results show completely wrong products since this morning.",
    "The app crashed right after I uploaded my profile photo.",
    "I can't see my order history anymore, it just shows a spinner.",
    "Got an error message about 'something went wrong' during payment.",
    "The notification settings page won't load at all.",
    "My cart items disappeared after I changed my shipping address.",
    "The export button downloads an empty file every time.",
    "I entered my credit card info and the page just went white.",
    "Can't log in with my Google account, keeps saying 'authentication failed'.",
    "The dashboard takes over 30 seconds to load now.",
    "Images are broken on every product page.",
    "I got charged twice for the same order!",
]

USER_NAMES = [
    "Jane Doe", "John Smith", "Maria Garcia", "Ahmed Hassan",
    "Yuki Tanaka", "Priya Sharma", "Alex Mueller", "Sofia Rossi",
    "Wei Chen", "Olga Petrova", "Carlos Lopez", "Emma Wilson",
]


# ---------------------------------------------------------------------------
# Attachment content templates
# ---------------------------------------------------------------------------

LOG_SNIPPETS = [
    """2024-01-15 14:32:01 INFO  Starting worker process pid=12345
2024-01-15 14:32:01 INFO  Connected to database
2024-01-15 14:32:05 WARN  Slow query detected: 2341ms
2024-01-15 14:32:06 ERROR Connection reset by peer
2024-01-15 14:32:06 ERROR Worker process crashed, restarting...""",
    """[2024-01-15T14:00:00Z] request_id=abc-123 method=POST path=/api/orders status=500 duration=1523ms
[2024-01-15T14:00:00Z] request_id=abc-123 error="database connection timeout"
[2024-01-15T14:00:01Z] request_id=abc-123 retry=1 status=500
[2024-01-15T14:00:03Z] request_id=abc-123 retry=2 status=200 duration=89ms""",
    """thread 'main' panicked at 'assertion failed: buffer.len() <= MAX_SIZE'
stack backtrace:
   0: std::panicking::begin_panic
   1: app::buffer::validate
   2: app::ingest::process
   3: tokio::runtime::task::harness
note: run with `RUST_BACKTRACE=1` for a full backtrace""",
    """Exception in thread "pool-3-thread-7" java.lang.OutOfMemoryError: Java heap space
\tat com.example.service.ReportService.generateLargeReport(ReportService.java:142)
\tat com.example.controller.ReportController.export(ReportController.java:58)
Heap dump written to /tmp/heapdump-20240115.hprof""",
]


# ---------------------------------------------------------------------------
# Stacktrace builder
# ---------------------------------------------------------------------------

INLINE_SOURCE_CONTEXT_PLATFORMS = {"python", "go", "node", "ruby", "php", "javascript", "java", "swift", "rust"}

_JAVASCRIPT_SNIPPETS = [
    (
        "    const user = await fetchUser(props.userId);",
        [
            "  useEffect(() => {",
            "    let cancelled = false;",
            "    async function loadData() {",
            "      setLoading(true);",
            "      try {",
            "        if (!cancelled) {",
            "          setUser(user);",
            "          setLoading(false);",
            "        }",
            "      } catch (err) {",
        ],
    ),
    (
        "    const el = document.querySelector(config.selector);",
        [
            "  function mountWidget(config) {",
            "    if (!config || !config.selector) {",
            "      throw new Error('Invalid widget configuration');",
            "    }",
            "",
            "    if (!el) {",
            "      throw new DOMException(`Element not found: ${config.selector}`);",
            "    }",
            "    ReactDOM.render(<App {...config} />, el);",
            "    return el;",
        ],
    ),
    (
        "    const res = await fetch(`/api/projects/${id}`, { headers });",
        [
            "  export async function getProject(id) {",
            "    const headers = {",
            "      'Authorization': `Bearer ${getToken()}`,",
            "      'Content-Type': 'application/json',",
            "    };",
            "    if (!res.ok) {",
            "      const body = await res.text();",
            "      throw new ApiError(res.status, body);",
            "    }",
            "    return res.json();",
        ],
    ),
    (
        "    const parsed = JSON.parse(event.body);",
        [
            "  export async function handler(event, context) {",
            "    if (!event.body) {",
            "      return { statusCode: 400, body: 'Missing body' };",
            "    }",
            "    try {",
            "      const validated = schema.validate(parsed);",
            "      return { statusCode: 200, body: JSON.stringify(validated) };",
            "    } catch (err) {",
            "      console.error('Parse error:', err);",
        ],
    ),
    (
        "    const { data, error } = useSWR(`/api/users/${userId}`, fetcher);",
        [
            "  export default function ProfilePage({ userId }) {",
            "    const router = useRouter();",
            "    const fetcher = (url) => fetch(url).then(r => r.json());",
            "",
            "",
            "    if (error) return <ErrorBoundary error={error} />;",
            "    if (!data) return <Skeleton />;",
            "",
            "    return (",
        ],
    ),
]

_PYTHON_SNIPPETS = [
    (
        "        result = cursor.execute(query, params)",
        [
            "    def execute_query(self, query, params=None):",
            '        """Execute a database query with optional parameters."""',
            "        cursor = self.connection.cursor()",
            "        try:",
            "            logger.debug(f\"Executing: {query}\")",
            "            self.connection.commit()",
            "            return cursor.fetchall()",
            "        except Exception as e:",
            "            self.connection.rollback()",
            "            raise",
        ],
    ),
    (
        "        user = User.objects.get(pk=user_id)",
        [
            "    def get_user(self, user_id):",
            '        """Fetch user by primary key."""',
            "        if not isinstance(user_id, int):",
            '            raise ValueError("user_id must be an integer")',
            "",
            "        if not user.is_active:",
            '            raise PermissionError("user account is disabled")',
            "        self._cache[user_id] = user",
            "        return user",
            "",
        ],
    ),
    (
        "        response = requests.post(url, json=payload, timeout=30)",
        [
            "    def send_request(self, url, payload):",
            "        headers = self._build_headers()",
            "        logger.info(f\"POST {url}\")",
            "        retries = 0",
            "        while retries < self.max_retries:",
            "            if response.status_code == 200:",
            "                return response.json()",
            "            retries += 1",
            "            time.sleep(2 ** retries)",
            '        raise RuntimeError("max retries exceeded")',
        ],
    ),
    (
        "        data = json.loads(raw_body)",
        [
            "    def parse_request(self, request):",
            "        content_type = request.headers.get('Content-Type', '')",
            "        if 'application/json' not in content_type:",
            "            raise ValueError('unsupported content type')",
            "        raw_body = request.body.decode('utf-8')",
            "        if not isinstance(data, dict):",
            "            raise TypeError('expected JSON object')",
            "        return self.validate(data)",
            "",
            "",
        ],
    ),
    (
        "        value = config[key]",
        [
            "    def get_config(self, key, default=None):",
            '        """Retrieve a config value, raising KeyError if missing."""',
            "        config = self._load_config()",
            "        if key not in config and default is None:",
            "            logger.warning(f\"Missing config key: {key}\")",
            "        return value",
            "",
            "    def _load_config(self):",
            "        with open(self.config_path) as f:",
            "            return yaml.safe_load(f)",
        ],
    ),
]

_GO_SNIPPETS = [
    (
        "\tresult, err := db.QueryContext(ctx, query, args...)",
        [
            "func (r *Repository) FindByID(ctx context.Context, id int64) (*Entity, error) {",
            '\tquery := "SELECT * FROM entities WHERE id = $1"',
            "\targs := []interface{}{id}",
            "",
            "\tlog.Printf(\"executing query for id=%d\", id)",
            "\tif err != nil {",
            "\t\treturn nil, fmt.Errorf(\"query failed: %w\", err)",
            "\t}",
            "\tdefer result.Close()",
            "",
        ],
    ),
    (
        "\tresp, err := client.Do(req)",
        [
            "func (c *Client) Call(ctx context.Context, method, path string, body io.Reader) (*http.Response, error) {",
            "\treq, err := http.NewRequestWithContext(ctx, method, c.baseURL+path, body)",
            "\tif err != nil {",
            "\t\treturn nil, err",
            "\t}",
            "\tif err != nil {",
            '\t\treturn nil, fmt.Errorf("request to %s failed: %w", path, err)',
            "\t}",
            "\tif resp.StatusCode >= 500 {",
            '\t\treturn resp, fmt.Errorf("server error: %d", resp.StatusCode)',
        ],
    ),
    (
        "\tval := items[idx]",
        [
            "func (s *Service) Process(items []Item) error {",
            "\tfor idx := range s.indices {",
            "\t\tif idx < 0 {",
            "\t\t\tcontinue",
            "\t\t}",
            "\t\tif err := s.handle(val); err != nil {",
            "\t\t\treturn err",
            "\t\t}",
            "\t}",
            "\treturn nil",
        ],
    ),
    (
        "\terr := json.NewDecoder(r.Body).Decode(&req)",
        [
            "func (h *Handler) HandleCreate(w http.ResponseWriter, r *http.Request) {",
            "\tvar req CreateRequest",
            "",
            "\tdefer r.Body.Close()",
            "",
            "\tif err != nil {",
            "\t\thttp.Error(w, err.Error(), http.StatusBadRequest)",
            "\t\treturn",
            "\t}",
            "",
        ],
    ),
    (
        "\tconn, err := tls.DialWithDialer(dialer, \"tcp\", addr, tlsConf)",
        [
            "func (c *Client) Connect(ctx context.Context) error {",
            "\tdialer := &net.Dialer{Timeout: c.timeout}",
            "\taddr := fmt.Sprintf(\"%s:%d\", c.host, c.port)",
            "",
            "\ttlsConf := c.buildTLSConfig()",
            "\tif err != nil {",
            '\t\treturn fmt.Errorf("tls connect to %s failed: %w", addr, err)',
            "\t}",
            "\tc.conn = conn",
            "\treturn nil",
        ],
    ),
]

_NODE_SNIPPETS = [
    (
        "    const result = await pool.query(queryText, values);",
        [
            "  async function executeQuery(queryText, values) {",
            "    const client = await pool.connect();",
            "    try {",
            "      logger.debug(`Executing: ${queryText}`);",
            "",
            "      return result.rows;",
            "    } catch (err) {",
            "      logger.error('Query failed', { queryText, err });",
            "      throw err;",
            "    } finally {",
        ],
    ),
    (
        "    const data = JSON.parse(body);",
        [
            "  async function parseBody(req) {",
            "    const chunks = [];",
            "    for await (const chunk of req) {",
            "      chunks.push(chunk);",
            "    }",
            "    if (!data || typeof data !== 'object') {",
            "      throw new TypeError('Expected JSON object');",
            "    }",
            "    return data;",
            "  }",
        ],
    ),
    (
        "    const user = await UserModel.findById(userId);",
        [
            "  async function getUser(userId) {",
            "    if (!userId) {",
            "      throw new Error('userId is required');",
            "    }",
            "",
            "    if (!user) {",
            "      throw new NotFoundError(`User ${userId} not found`);",
            "    }",
            "    return user.toJSON();",
            "  }",
        ],
    ),
    (
        "    const response = await fetch(url, { signal: controller.signal });",
        [
            "  async function fetchWithTimeout(url, timeoutMs = 5000) {",
            "    const controller = new AbortController();",
            "    const timer = setTimeout(() => controller.abort(), timeoutMs);",
            "",
            "    try {",
            "      clearTimeout(timer);",
            "      if (!response.ok) {",
            "        throw new Error(`HTTP ${response.status}`);",
            "      }",
            "      return await response.json();",
        ],
    ),
    (
        "    const decoded = jwt.verify(token, secret);",
        [
            "  function verifyToken(req, res, next) {",
            "    const header = req.headers.authorization;",
            "    if (!header) return res.status(401).json({ error: 'missing token' });",
            "    const token = header.split(' ')[1];",
            "",
            "    req.user = decoded;",
            "    next();",
            "  }",
            "",
            "  module.exports = { verifyToken };",
        ],
    ),
]

_RUBY_SNIPPETS = [
    (
        "        user = User.find(params[:id])",
        [
            "  def show",
            "    authorize! :read, User",
            "",
            "    begin",
            "      @cache_key = \"user_#{params[:id]}\"",
            "      @profile = user.profile",
            "      render json: UserSerializer.new(user)",
            "    rescue ActiveRecord::RecordNotFound => e",
            "      render json: { error: e.message }, status: :not_found",
            "    end",
        ],
    ),
    (
        "        result = connection.execute(sql)",
        [
            "  def execute_query(sql, binds = [])",
            "    ActiveRecord::Base.connection_pool.with_connection do |connection|",
            "      Rails.logger.debug { \"SQL: #{sql}\" }",
            "      binds.each_with_index do |val, i|",
            "        sql = sql.gsub(\"$#{i + 1}\", connection.quote(val))",
            "        result.to_a",
            "      end",
            "    end",
            "  rescue ActiveRecord::StatementInvalid => e",
            "    Rails.logger.error(\"Query failed: #{e.message}\")",
        ],
    ),
    (
        "        response = HTTParty.post(url, body: payload.to_json, headers: headers)",
        [
            "  def send_webhook(url, payload)",
            "    headers = { 'Content-Type' => 'application/json' }",
            "    headers['Authorization'] = \"Bearer #{api_token}\"",
            "",
            "    retries = 0",
            "    unless response.success?",
            "      raise ApiError, \"webhook failed: #{response.code}\"",
            "    end",
            "",
            "    JSON.parse(response.body)",
        ],
    ),
    (
        "        data = JSON.parse(raw_body)",
        [
            "  def parse_request(request)",
            "    raw_body = request.body.read",
            "    if raw_body.blank?",
            "      raise ActionController::BadRequest, 'empty body'",
            "    end",
            "    data.deep_symbolize_keys",
            "  rescue JSON::ParserError => e",
            "    raise ActionController::BadRequest, \"invalid JSON: #{e.message}\"",
            "  end",
            "",
        ],
    ),
    (
        "        worker.perform_async(user.id, opts)",
        [
            "  def enqueue_job(user, opts = {})",
            "    unless user.persisted?",
            "      raise ArgumentError, 'user must be saved first'",
            "    end",
            "",
            "    Rails.logger.info(\"Enqueued job for user #{user.id}\")",
            "    { job_id: worker.jid, status: :queued }",
            "  end",
            "",
            "  private",
        ],
    ),
]

_PHP_SNIPPETS = [
    (
        "        $result = $this->connection->execute($query, $bindings);",
        [
            "    public function executeQuery(string $query, array $bindings = []): array",
            "    {",
            "        $this->logger->debug('Executing query', ['sql' => $query]);",
            "",
            "        try {",
            "            return $result->fetchAllAssociative();",
            "        } catch (\\Doctrine\\DBAL\\Exception $e) {",
            "            $this->logger->error('Query failed', ['error' => $e->getMessage()]);",
            "            throw $e;",
            "        }",
        ],
    ),
    (
        "        $user = User::findOrFail($id);",
        [
            "    public function show(int $id): JsonResponse",
            "    {",
            "        $this->authorize('view', User::class);",
            "",
            "        /** @var User $user */",
            "        return response()->json(new UserResource($user));",
            "    }",
            "",
            "    public function update(Request $request, int $id): JsonResponse",
            "    {",
        ],
    ),
    (
        "        $response = Http::timeout(30)->post($url, $payload);",
        [
            "    public function sendRequest(string $url, array $payload): array",
            "    {",
            "        $headers = $this->buildHeaders();",
            "        Log::info('POST ' . $url);",
            "",
            "        if ($response->failed()) {",
            "            throw new \\RuntimeException('Request failed: ' . $response->status());",
            "        }",
            "",
            "        return $response->json();",
        ],
    ),
    (
        "        $data = json_decode($body, true, 512, JSON_THROW_ON_ERROR);",
        [
            "    public function parseBody(Request $request): array",
            "    {",
            "        $body = $request->getContent();",
            "        if (empty($body)) {",
            "            throw new BadRequestHttpException('Empty request body');",
            "        if (!is_array($data)) {",
            "            throw new BadRequestHttpException('Expected JSON object');",
            "        }",
            "",
            "        return $data;",
        ],
    ),
    (
        "        Cache::put($key, $value, $ttl);",
        [
            "    public function cacheResult(string $key, mixed $value, int $ttl = 3600): void",
            "    {",
            "        if (empty($key)) {",
            "            throw new \\InvalidArgumentException('Cache key cannot be empty');",
            "        }",
            "        Log::debug('Cached', ['key' => $key, 'ttl' => $ttl]);",
            "    }",
            "",
            "    public function invalidate(string $key): void",
            "    {",
        ],
    ),
]

_JAVA_SNIPPETS = [
    (
        "        User user = userRepository.findById(userId).orElseThrow();",
        [
            "    @Transactional(readOnly = true)",
            "    public UserDTO getUser(Long userId) {",
            "        log.debug(\"Fetching user with id={}\", userId);",
            "",
            "        try {",
            "            return UserMapper.toDTO(user);",
            "        } catch (NoSuchElementException e) {",
            "            throw new ResourceNotFoundException(\"User not found: \" + userId);",
            "        }",
            "    }",
        ],
    ),
    (
        "        ResultSet rs = stmt.executeQuery();",
        [
            "    private List<Record> executeRawQuery(String sql, Object... params) throws SQLException {",
            "        try (Connection conn = dataSource.getConnection();",
            "             PreparedStatement stmt = conn.prepareStatement(sql)) {",
            "            for (int i = 0; i < params.length; i++) {",
            "                stmt.setObject(i + 1, params[i]);",
            "            List<Record> records = new ArrayList<>();",
            "            while (rs.next()) {",
            "                records.add(mapRow(rs));",
            "            }",
            "            return records;",
        ],
    ),
    (
        "        ResponseEntity<String> response = restTemplate.exchange(url, HttpMethod.POST, entity, String.class);",
        [
            "    public String callExternalService(String url, Map<String, Object> payload) {",
            "        HttpHeaders headers = new HttpHeaders();",
            "        headers.setContentType(MediaType.APPLICATION_JSON);",
            "        HttpEntity<Map<String, Object>> entity = new HttpEntity<>(payload, headers);",
            "",
            "        if (!response.getStatusCode().is2xxSuccessful()) {",
            "            throw new ExternalServiceException(\"Call failed: \" + response.getStatusCode());",
            "        }",
            "        return response.getBody();",
            "    }",
        ],
    ),
    (
        "        Object bean = context.getBean(beanName);",
        [
            "    @Override",
            "    public void onApplicationEvent(ContextRefreshedEvent event) {",
            "        ApplicationContext context = event.getApplicationContext();",
            "        for (String beanName : context.getBeanDefinitionNames()) {",
            "            try {",
            "                if (bean instanceof InitializingBean) {",
            "                    log.info(\"Initialized bean: {}\", beanName);",
            "                }",
            "            } catch (BeansException e) {",
            "                log.error(\"Failed to initialize bean: {}\", beanName, e);",
        ],
    ),
    (
        "        byte[] bytes = objectMapper.writeValueAsBytes(event);",
        [
            "    @KafkaListener(topics = \"${app.kafka.topic}\")",
            "    public void handleEvent(ConsumerRecord<String, String> record) {",
            "        log.info(\"Received event key={}\", record.key());",
            "        DomainEvent event = parseEvent(record.value());",
            "",
            "        kafkaTemplate.send(dlqTopic, record.key(), new String(bytes));",
            "        eventStore.save(event);",
            "        log.debug(\"Processed event: {}\", event.getId());",
            "    }",
            "",
        ],
    ),
]

_SWIFT_SNIPPETS = [
    (
        "        let user = try await userService.fetchUser(id: userId)",
        [
            "    func loadProfile() async throws {",
            "        guard let userId = Auth.currentUser?.id else {",
            '            throw AppError.unauthorized("No authenticated user")',
            "        }",
            "        isLoading = true",
            "        self.user = user",
            "        self.isLoading = false",
            "    }",
            "",
            "    func refreshProfile() async {",
        ],
    ),
    (
        "        let data = try JSONDecoder().decode(ResponseDTO.self, from: responseData)",
        [
            "    func fetchData(from url: URL) async throws -> ResponseDTO {",
            "        let (responseData, response) = try await URLSession.shared.data(from: url)",
            "        guard let httpResponse = response as? HTTPURLResponse,",
            "              httpResponse.statusCode == 200 else {",
            '            throw NetworkError.badResponse(url: url.absoluteString)',
            "        return data",
            "    }",
            "",
            "    private func buildURL(path: String) throws -> URL {",
            '        guard let url = URL(string: baseURL + path) else {',
        ],
    ),
    (
        "        let result = try context.fetch(fetchRequest)",
        [
            "    func fetchItems(predicate: NSPredicate? = nil) throws -> [Item] {",
            "        let fetchRequest: NSFetchRequest<Item> = Item.fetchRequest()",
            "        fetchRequest.predicate = predicate",
            "        fetchRequest.sortDescriptors = [NSSortDescriptor(key: \"createdAt\", ascending: false)]",
            "",
            "        return result",
            "    }",
            "",
            "    func saveContext() throws {",
            "        guard context.hasChanges else { return }",
        ],
    ),
    (
        "        let snapshot = try await document.reference.getDocument()",
        [
            "    func syncDocument(_ document: Document) async throws {",
            "        guard document.isDirty else { return }",
            "        let reference = db.collection(\"documents\").document(document.id)",
            "",
            "        do {",
            "            let remoteData = snapshot.data() ?? [:]",
            "            if let remoteTimestamp = remoteData[\"updatedAt\"] as? Timestamp,",
            "               remoteTimestamp.dateValue() > document.updatedAt {",
            '                throw SyncError.conflict(documentId: document.id)',
            "            }",
        ],
    ),
    (
        "        let image = try await imageLoader.load(url: url)",
        [
            "    @MainActor",
            "    func updateAvatar(url: URL) async throws {",
            "        avatarState = .loading",
            "",
            "        do {",
            "            avatarState = .loaded(image)",
            "        } catch {",
            "            avatarState = .failed(error)",
            "            throw error",
            "        }",
        ],
    ),
]

_RUST_SNIPPETS = [
    (
        "    let user = sqlx::query_as!(User, \"SELECT * FROM users WHERE id = $1\", id)",
        [
            "async fn get_user(",
            "    State(pool): State<PgPool>,",
            "    Path(id): Path<i64>,",
            ") -> Result<Json<User>, AppError> {",
            "",
            "        .fetch_one(&pool)",
            "        .await",
            "        .map_err(|e| AppError::NotFound(format!(\"User {} not found: {}\", id, e)))?;",
            "",
            "    Ok(Json(user))",
        ],
    ),
    (
        "    let body: CreateRequest = serde_json::from_slice(&bytes)?;",
        [
            "async fn create_item(",
            "    State(pool): State<PgPool>,",
            "    bytes: Bytes,",
            ") -> Result<Json<Item>, AppError> {",
            "",
            "    let item = sqlx::query_as!(Item,",
            '        "INSERT INTO items (name, data) VALUES ($1, $2) RETURNING *",',
            "        body.name,",
            "        body.data,",
            "    )",
        ],
    ),
    (
        "    let response = client.get(&url).send().await?.error_for_status()?;",
        [
            "async fn fetch_remote_data(",
            "    client: &reqwest::Client,",
            "    base_url: &str,",
            "    path: &str,",
            ") -> Result<RemoteData, anyhow::Error> {",
            "    let data: RemoteData = response.json().await?;",
            "    tracing::debug!(path, \"fetched remote data\");",
            "    Ok(data)",
            "}",
            "",
        ],
    ),
    (
        "    let conn = pool.get().await.map_err(|e| AppError::Internal(e.to_string()))?;",
        [
            "async fn health_check(",
            "    State(pool): State<deadpool_postgres::Pool>,",
            ") -> Result<StatusCode, AppError> {",
            "",
            "",
            '    conn.execute("SELECT 1", &[])',
            "        .await",
            '        .map_err(|e| AppError::Internal(format!("db check failed: {}", e)))?;',
            "",
            "    Ok(StatusCode::OK)",
        ],
    ),
    (
        "    let claims = decode::<Claims>(&token, &decoding_key, &validation)?;",
        [
            "async fn auth_middleware(",
            "    State(config): State<AppConfig>,",
            "    mut req: Request<Body>,",
            "    next: Next<Body>,",
            ") -> Result<Response, AppError> {",
            "    let user_id = claims.claims.sub;",
            "    req.extensions_mut().insert(UserId(user_id));",
            "    Ok(next.run(req).await)",
            "}",
            "",
        ],
    ),
]

_SOURCE_CONTEXT_SNIPPETS = {
    "python": _PYTHON_SNIPPETS,
    "go": _GO_SNIPPETS,
    "node": _NODE_SNIPPETS,
    "ruby": _RUBY_SNIPPETS,
    "php": _PHP_SNIPPETS,
    "javascript": _JAVASCRIPT_SNIPPETS,
    "java": _JAVA_SNIPPETS,
    "swift": _SWIFT_SNIPPETS,
    "rust": _RUST_SNIPPETS,
}


def _pick_source_context(platform, lineno):
    """Return (pre_context, context_line, post_context) for a frame."""
    snippets = _SOURCE_CONTEXT_SNIPPETS.get(platform)
    if not snippets:
        return None, None, None
    context_line, surrounding = random.choice(snippets)
    pre_context = surrounding[:5]
    post_context = surrounding[5:]
    return pre_context, context_line, post_context


_VARS_POOL = {
    "python": [
        {"self": "<UserService>", "user_id": "42", "query": "SELECT * FROM users WHERE id = %s"},
        {"self": "<PaymentProcessor>", "amount": "99.95", "currency": "USD", "retries": "0"},
        {"request": "<WSGIRequest: POST '/api/v1/items'>", "data": "{'name': 'widget'}", "user": "<User: alice>"},
        {"self": "<CacheBackend>", "key": "session:abc123", "ttl": "3600"},
        {"conn": "<psycopg2.connection>", "cursor": "<cursor object>", "params": "('active',)"},
    ],
    "javascript": [
        {"this": "[HTMLDivElement]", "props": "{ userId: 42, name: 'John' }", "event": "ClickEvent"},
        {"this": "Window", "url": "'/api/dashboard'", "controller": "AbortController"},
        {"state": "{ loading: true, error: null }", "dispatch": "[Function]", "action": "{ type: 'FETCH' }"},
        {"el": "null", "config": "{ selector: '#root', debug: false }"},
        {"token": "'eyJhbGciOiJIUz...'", "decoded": "undefined", "secret": "'[REDACTED]'"},
    ],
    "node": [
        {"this": "[HTTPServer]", "req": "IncomingMessage", "res": "ServerResponse"},
        {"pool": "BoundPool { totalCount: 10 }", "queryText": "SELECT * FROM orders", "values": "[42]"},
        {"err": "null", "body": "{ id: 42, status: 'pending' }", "chunks": "[ Buffer(1024) ]"},
        {"this": "[Router]", "path": "'/api/v2/users/:id'", "middleware": "[Function: auth]"},
        {"client": "RedisClient { connected: true }", "key": "'sess:xyz'", "ttl": "7200"},
    ],
    "java": [
        {"this": "UserService@3f2a1b", "userId": "42L", "connection": "HikariProxyConnection@7e2c"},
        {"this": "OrderController@5d1ef9", "request": "HttpServletRequest@2a4b", "response": "HttpServletResponse@6c3d"},
        {"stmt": "PreparedStatement@4e9a", "rs": "null", "sql": "SELECT * FROM items WHERE status = ?"},
        {"this": "KafkaConsumer@8b1c", "record": "ConsumerRecord(topic=events, offset=1042)", "event": "DomainEvent@3af2"},
        {"bean": "null", "context": "AnnotationConfigApplicationContext@7d1f", "beanName": "userService"},
    ],
    "ruby": [
        {"self": "#<UsersController:0x00007f>", "params": "{ id: \"42\" }", "@user": "nil"},
        {"self": "#<OrderService:0x00007fa2>", "order": "#<Order id: 17>", "total": "BigDecimal('149.99')"},
        {"connection": "#<ActiveRecord::ConnectionAdapters::PostgreSQLAdapter>", "sql": "SELECT * FROM sessions", "result": "nil"},
        {"self": "#<WebhookWorker:0x00007f>", "url": "\"https://hooks.example.com\"", "payload": "{ event: \"order.created\" }"},
        {"raw_body": "\"{\\\"name\\\":\\\"test\\\"}\"", "data": "nil", "request": "#<ActionDispatch::Request>"},
    ],
    "go": [
        {"ctx": "context.Background.WithCancel.WithValue", "err": "connection refused", "retries": "3"},
        {"req": "&http.Request{Method:\"POST\"}", "w": "http.ResponseWriter", "body": "[]byte(nil)"},
        {"conn": "*pgx.Conn", "query": "SELECT * FROM users WHERE active = $1", "args": "[]interface{}{true}"},
        {"dialer": "&net.Dialer{Timeout: 5s}", "addr": "\"db.example.com:5432\"", "tlsConf": "*tls.Config"},
        {"items": "[]Item(nil)", "idx": "7", "val": "Item{}"},
    ],
    "php": [
        {"$this": "App\\Services\\UserService", "$userId": "42", "$result": "null"},
        {"$this": "App\\Http\\Controllers\\OrderController", "$request": "Illuminate\\Http\\Request", "$id": "17"},
        {"$query": "SELECT * FROM products WHERE active = 1", "$bindings": "[]", "$connection": "MySqlConnection"},
        {"$key": "\"cache:user:42\"", "$value": "null", "$ttl": "3600"},
        {"$body": "\"{\\\"event\\\":\\\"order.paid\\\"}\"", "$data": "null", "$url": "\"https://api.example.com/hook\""},
    ],
    "swift": [
        {"self": "ProfileViewModel", "userId": "Optional(42)", "error": "nil"},
        {"self": "NetworkService", "url": "URL(\"https://api.example.com/v1/users\")", "responseData": "0 bytes"},
        {"fetchRequest": "NSFetchRequest<Item>", "predicate": "nil", "result": "[]"},
        {"self": "SyncManager", "document": "Document(id: \"abc123\")", "snapshot": "nil"},
        {"self": "AvatarLoader", "url": "URL(\"https://cdn.example.com/avatar.jpg\")", "image": "nil"},
    ],
    "rust": [
        {"self": "&UserHandler", "user_id": "42", "pool": "Pool { size: 10, available: 0 }"},
        {"bytes": "Bytes(len=256)", "body": "CreateRequest { name: \"test\" }", "pool": "PgPool"},
        {"client": "&Client", "url": "\"https://api.example.com/data\"", "response": "<pending>"},
        {"conn": "Object { .. }", "pool": "Pool { size: 5, available: 2 }"},
        {"token": "\"eyJhbGci...\"", "claims": "Claims { sub: 42, exp: 1700000000 }", "config": "&AppConfig"},
    ],
}


def _make_vars(platform):
    """Return a dict of 2-4 realistic local variables for the given platform."""
    pool = _VARS_POOL.get(platform)
    if not pool:
        return {}
    source = random.choice(pool)
    keys = list(source.keys())
    count = random.randint(2, min(4, len(keys)))
    selected = random.sample(keys, count)
    return {k: source[k] for k in selected}


_EXTRA_FRAMEWORK_FRAMES = {
    "python": [
        ("django/core/handlers/base.py", "_get_response", "django.core.handlers.base"),
        ("django/middleware/common.py", "__call__", "django.middleware.common"),
        ("django/utils/deprecation.py", "__call__", "django.utils.deprecation"),
        ("django/core/handlers/exception.py", "inner", "django.core.handlers.exception"),
        ("django/middleware/security.py", "process_request", "django.middleware.security"),
    ],
    "node": [
        ("node_modules/express/lib/router/layer.js", "Layer.handle", "express"),
        ("node_modules/express/lib/router/route.js", "Route.dispatch", "express"),
        ("node_modules/express/lib/router/index.js", "process_params", "express"),
        ("node_modules/body-parser/lib/read.js", "read", "body-parser"),
        ("node_modules/express/lib/application.js", "handle", "express"),
    ],
    "javascript": [
        ("webpack-internal:///./node_modules/react-dom/cjs/react-dom.development.js", "commitWork", "react-dom"),
        ("webpack-internal:///./node_modules/react/cjs/react.development.js", "dispatchAction", "react"),
        ("webpack-internal:///./node_modules/scheduler/cjs/scheduler.development.js", "flushWork", "scheduler"),
        ("webpack-internal:///./node_modules/next/dist/client/router.js", "Router.change", "next"),
    ],
    "ruby": [
        ("rack/lib/rack/runtime.rb", "call", "Rack::Runtime"),
        ("actionpack/lib/action_dispatch/middleware/executor.rb", "call", "ActionDispatch::Executor"),
        ("railties/lib/rails/rack/logger.rb", "call_app", "Rails::Rack::Logger"),
        ("activesupport/lib/active_support/cache/strategy/local_cache_middleware.rb", "call", "ActiveSupport::Cache"),
        ("actionpack/lib/action_dispatch/routing/route_set.rb", "dispatch", "ActionDispatch::Routing::RouteSet"),
    ],
    "java": [
        ("org/springframework/web/filter/OncePerRequestFilter.java", "doFilterInternal", "org.springframework.web.filter"),
        ("org/springframework/web/servlet/DispatcherServlet.java", "doDispatch", "org.springframework.web.servlet"),
        ("org/apache/catalina/core/ApplicationFilterChain.java", "doFilter", "org.apache.catalina.core"),
        ("org/springframework/security/web/FilterChainProxy.java", "doFilter", "org.springframework.security.web"),
        ("org/springframework/web/servlet/FrameworkServlet.java", "service", "org.springframework.web.servlet"),
    ],
    "go": [
        ("net/http/server.go", "(*conn).serve", "net/http"),
        ("net/http/server.go", "serverHandler.ServeHTTP", "net/http"),
        ("github.com/gorilla/mux/mux.go", "(*Router).ServeHTTP", "github.com/gorilla/mux"),
        ("net/http/server.go", "HandlerFunc.ServeHTTP", "net/http"),
    ],
    "php": [
        ("vendor/laravel/framework/src/Illuminate/Pipeline/Pipeline.php", "Illuminate\\Pipeline\\Pipeline::then", "Illuminate\\Pipeline"),
        ("vendor/laravel/framework/src/Illuminate/Routing/Router.php", "Illuminate\\Routing\\Router::dispatch", "Illuminate\\Routing"),
        ("vendor/laravel/framework/src/Illuminate/Foundation/Http/Kernel.php", "Illuminate\\Foundation\\Http\\Kernel::handle", "Illuminate\\Foundation\\Http"),
        ("vendor/laravel/framework/src/Illuminate/Routing/Middleware/SubstituteBindings.php", "handle", "Illuminate\\Routing\\Middleware"),
    ],
    "swift": [
        ("UIKitCore/UIApplication.swift", "UIApplication.main", "UIKitCore"),
        ("UIKitCore/UIScene.swift", "UIScene._callConnectionHandler", "UIKitCore"),
        ("SwiftUI/SwiftUI.framework/View.swift", "ViewGraph.updateOutputs", "SwiftUI"),
        ("libdispatch/queue.c", "_dispatch_main_queue_drain", "libdispatch"),
    ],
    "rust": [
        ("tokio/src/runtime/scheduler/multi_thread/worker.rs", "tokio::runtime::task::harness::poll", "tokio"),
        ("hyper/src/proto/h1/dispatch.rs", "hyper::proto::h1::dispatch::Dispatcher::poll_inner", "hyper"),
        ("axum/src/routing/route.rs", "axum::routing::route::Route::call", "axum"),
        ("tower/src/util/service_fn.rs", "tower::util::service_fn::ServiceFn::call", "tower"),
    ],
}


def _abs_path(filename, platform, in_app):
    """Return an absolute path for a stack frame filename."""
    if platform == "python":
        if in_app:
            return "/app/" + filename
        return "/usr/lib/python3.12/" + filename
    elif platform == "node":
        if in_app:
            return "/app/" + filename
        return "/app/" + filename  # node_modules already in filename
    elif platform == "javascript":
        return filename  # URLs stay as-is
    elif platform == "ruby":
        if in_app:
            return "/app/" + filename
        return "/usr/lib/ruby/gems/3.3.0/gems/" + filename
    elif platform == "java":
        return filename  # module path as-is
    elif platform == "go":
        if in_app:
            return "/app/" + filename
        return filename  # stdlib paths stay as-is
    elif platform == "php":
        if in_app:
            return "/var/www/html/" + filename
        return "/var/www/html/" + filename  # vendor already in filename
    elif platform == "swift":
        return filename
    elif platform == "rust":
        if in_app:
            return "/app/" + filename
        return "$CARGO_HOME/registry/src/" + filename
    return filename


def make_stacktrace(frames_spec, platform):
    """Build a stacktrace from a list of (filename, function, module) tuples."""
    has_source_context = platform in INLINE_SOURCE_CONTEXT_PLATFORMS

    # Optionally inject extra framework frames at the bottom of the stack
    if random.random() < 0.5:
        extra_pool = _EXTRA_FRAMEWORK_FRAMES.get(platform, [])
        if extra_pool:
            count = random.randint(2, min(3, len(extra_pool)))
            extra = random.sample(extra_pool, count)
            frames_spec = list(extra) + list(frames_spec)

    frames = []
    # in_app threshold: last 2 frames of the original spec are in_app,
    # but extra framework frames pushed to front are never in_app
    in_app_start = len(frames_spec) - 2
    for i, (filename, function, module) in enumerate(frames_spec):
        lineno = random.randint(10, 500)
        in_app = i >= in_app_start
        frame = {
            "filename": filename,
            "function": function,
            "lineno": lineno,
            "colno": random.randint(1, 80) if platform in ("javascript", "node") else None,
            "in_app": in_app,
            "abs_path": _abs_path(filename, platform, in_app),
        }
        if module:
            frame["module"] = module
        if frame["colno"] is None:
            del frame["colno"]
        if has_source_context:
            pre, ctx, post = _pick_source_context(platform, lineno)
            if ctx:
                frame["context_line"] = ctx
                frame["pre_context"] = pre
                frame["post_context"] = post
        # Add local variables to ~40% of in_app frames
        if in_app and random.random() < 0.4:
            vars_data = _make_vars(platform)
            if vars_data:
                frame["vars"] = vars_data
        frames.append(frame)
    return {"frames": frames}


# ---------------------------------------------------------------------------
# User builder
# ---------------------------------------------------------------------------

def make_user():
    uid = str(random.randint(1000, 9999))
    username = random.choice(USERNAMES)
    geo = random.choice(GEO_DATA)
    return {
        "id": uid,
        "email": f"user{random.randint(1, 500)}@example.com",
        "username": username,
        "ip_address": f"{random.randint(1,223)}.{random.randint(0,255)}.{random.randint(0,255)}.{random.randint(1,254)}",
        "geo": geo,
    }


# ---------------------------------------------------------------------------
# Extra context builder
# ---------------------------------------------------------------------------

def make_extra():
    chosen = random.sample(EXTRA_KEYS, random.randint(2, 3))
    return {k: fn() for k, fn in chosen}


# ---------------------------------------------------------------------------
# SDK builder with integrations and packages
# ---------------------------------------------------------------------------

def make_sdk(platform):
    sdk_name, sdk_version = SDK_BY_PLATFORM[platform]
    integrations = SDK_INTEGRATIONS.get(platform, [])
    packages = SDK_PACKAGES.get(platform, [])
    sdk = {
        "name": sdk_name,
        "version": sdk_version,
        "integrations": random.sample(integrations, min(len(integrations), random.randint(2, 5))),
        "packages": packages,
    }
    return sdk


# ---------------------------------------------------------------------------
# Event generators
# ---------------------------------------------------------------------------

def make_error_event(project, error_pattern=None):
    platform = project["platform"]
    event_id = rand_event_id()
    event_ts = rand_timestamp()

    if error_pattern is None:
        error_pattern = random.choice(ERROR_CATALOG[platform])

    exc_type, exc_value, frames_spec, mech_type, handled = error_pattern

    # ~20% chance of chained exceptions
    chained = random.random() < 0.20
    exc_values = []
    if chained:
        root_errors = ERROR_CATALOG[platform]
        root_pattern = random.choice(root_errors)
        r_type, r_value, r_frames, _, _ = root_pattern
        if r_value is None:
            r_value = r_type
        exc_values.append({
            "type": r_type,
            "value": r_value,
            "mechanism": {"type": "chained", "handled": handled},
            "stacktrace": make_stacktrace(r_frames, platform),
        })

    if exc_value is None:
        exc_value = exc_type
    exc_values.append({
        "type": exc_type,
        "value": exc_value,
        "mechanism": {"type": mech_type, "handled": handled},
        "stacktrace": make_stacktrace(frames_spec, platform),
    })

    request_ctx = make_request_context(platform, project["name"])
    contexts = make_contexts(platform)
    server_name = random.choice(SERVER_NAMES)
    transaction_name = random.choice(TRANSACTIONS_BY_PLATFORM.get(platform, ["GET /unknown"]))

    # Add trace context to every error
    trace_id = rand_hex(32)
    span_id = rand_hex(16)
    contexts["trace"] = {
        "trace_id": trace_id,
        "span_id": span_id,
        "op": "http.server",
        "status": "internal_error",
    }

    fingerprint = random.choice([
        ["{{ default }}"],
        ["{{ default }}"],
        ["{{ default }}"],
        [exc_type, exc_value[:40] if exc_value else ""],
        ["{{ default }}", transaction_name],
    ])

    event = {
        "event_id": event_id,
        "timestamp": event_ts,
        "level": random.choice(["error", "error", "error", "fatal", "warning"]),
        "platform": platform,
        "logger": random.choice(LOGGERS_BY_PLATFORM.get(platform, ["root"])),
        "transaction": transaction_name,
        "release": f"{project['name']}@{random.choice(RELEASES)}",
        "dist": rand_hex(8),
        "environment": random.choice(ENVIRONMENTS),
        "sdk": make_sdk(platform),
        "exception": {"values": exc_values},
        "breadcrumbs": make_breadcrumbs(platform, event_ts),
        "contexts": contexts,
        "tags": make_tags(platform, project, handled, transaction_name),
        "user": make_user(),
        "fingerprint": fingerprint,
        "extra": make_extra(),
    }

    modules = MODULES_BY_PLATFORM.get(platform)
    if modules:
        event["modules"] = modules

    if server_name:
        event["server_name"] = server_name
    if request_ctx:
        event["request"] = request_ctx

    return event_id, event


def make_message_event(project):
    platform = project["platform"]
    event_id = rand_event_id()
    event_ts = rand_timestamp()
    messages = MESSAGES_BY_PLATFORM.get(platform, ["Unhandled event"])

    event = {
        "event_id": event_id,
        "timestamp": event_ts,
        "level": random.choice(LEVELS),
        "platform": platform,
        "logger": random.choice(LOGGERS_BY_PLATFORM.get(platform, ["root"])),
        "release": f"{project['name']}@{random.choice(RELEASES)}",
        "environment": random.choice(ENVIRONMENTS),
        "sdk": make_sdk(platform),
        "message": random.choice(messages),
        "breadcrumbs": make_breadcrumbs(platform, event_ts),
        "contexts": make_contexts(platform),
        "tags": make_tags(platform, project, True),
    }

    server_name = random.choice(SERVER_NAMES)
    if server_name:
        event["server_name"] = server_name

    return event_id, event


def make_transaction(project, trace_id=None, parent_span_id=None):
    platform = project["platform"]
    event_id = rand_event_id()
    start = rand_timestamp()
    duration_ms = random.randint(5, 8000)
    txn_names = TRANSACTIONS_BY_PLATFORM.get(platform, ["GET /api/v1/unknown"])

    if trace_id is None:
        trace_id = rand_hex(32)
    root_span_id = rand_hex(16)

    spans = []
    span_ops = {
        "javascript": ["browser", "resource.script", "resource.css", "http.client", "ui.render"],
        "python":     ["db.query", "http.client", "cache.get", "serialize", "celery.task"],
        "node":       ["db.query", "http.client", "cache.get", "serialize", "fs.read"],
        "ruby":       ["db.query", "http.client", "cache.get", "view.render", "activerecord"],
        "java":       ["db.query", "http.client", "serialize", "spring.controller", "kafka.send"],
        "go":         ["db.query", "http.client", "grpc.client", "cache.get"],
        "php":        ["db.query", "http.client", "cache.get", "view.render", "queue.publish"],
        "swift":      ["http.client", "db.query", "ui.load", "file.read", "app.lifecycle"],
        "rust":       ["db.query", "http.client", "serialize", "cache.get", "task.spawn"],
    }
    descs = {
        "db.query": ["SELECT * FROM users WHERE id = $1", "INSERT INTO orders (...)", "UPDATE inventory SET stock = stock - 1"],
        "http.client": ["GET https://api.stripe.com/v1/charges", "POST https://auth.internal/verify", "GET https://cdn.example.com/config.json"],
        "cache.get": ["Redis GET session:abc", "Redis GET user:42:profile", "Memcached GET product:100"],
        "serialize": ["JSON serialize response", "Protobuf encode message", "MessagePack serialize payload"],
        "grpc.client": ["grpc /auth.AuthService/Verify", "grpc /inventory.Stock/Check"],
        "browser": ["domContentLoaded", "loadEvent"],
        "resource.script": ["https://cdn.example.com/app.js"],
        "resource.css": ["https://cdn.example.com/styles.css"],
        "ui.render": ["React.render <Dashboard />"],
        "celery.task": ["app.tasks.send_email"],
        "fs.read": ["readFile /tmp/uploads/doc.pdf"],
        "view.render": ["users/show.html.erb", "orders/index.html.erb"],
        "activerecord": ["User.find(42)", "Order.where(status: :pending)"],
        "spring.controller": ["UserController.getUser", "OrderController.createOrder"],
        "kafka.send": ["produce topic=orders-events"],
        "queue.publish": ["dispatch App\\Jobs\\ProcessOrder"],
        "ui.load": ["UIViewController.viewDidLoad"],
        "file.read": ["FileManager.read /var/data/cache"],
        "app.lifecycle": ["applicationDidBecomeActive"],
        "task.spawn": ["tokio::spawn process_batch"],
    }

    num_spans = random.randint(1, 5)
    span_ids = []
    for _ in range(num_spans):
        op = random.choice(span_ops.get(platform, ["db.query"]))
        desc_list = descs.get(op, [op])
        span_id = rand_hex(16)
        span_ids.append(span_id)
        span_start = start + random.uniform(0, duration_ms / 2000)
        span_dur = random.uniform(0.001, duration_ms / 1000 * 0.8)
        spans.append({
            "span_id": span_id,
            "trace_id": trace_id,
            "parent_span_id": root_span_id,
            "op": op,
            "description": random.choice(desc_list),
            "start_timestamp": span_start,
            "timestamp": span_start + span_dur,
            "status": random.choice(["ok", "ok", "ok", "ok", "internal_error"]),
        })

    txn_name = random.choice(txn_names)
    op = random.choice(["http.server", "http.client", "task", "ui.load"])

    # transaction_info source
    source_choices = {"javascript": "url", "swift": "view", "python": "route", "node": "route"}
    source = source_choices.get(platform, random.choice(["route", "url", "view", "task"]))

    measurements = {
        "duration": {"value": duration_ms, "unit": "millisecond"},
    }

    # Platform-specific measurements
    if platform == "javascript":
        measurements["fp"] = {"value": random.randint(50, 3000), "unit": "millisecond"}
        measurements["fcp"] = {"value": random.randint(50, 3000), "unit": "millisecond"}
        measurements["lcp"] = {"value": random.randint(100, 5000), "unit": "millisecond"}
        measurements["fid"] = {"value": random.randint(1, 300), "unit": "millisecond"}
        measurements["cls"] = {"value": round(random.uniform(0.0, 0.5), 3), "unit": ""}
        measurements["ttfb"] = {"value": random.randint(10, 2000), "unit": "millisecond"}
    elif platform == "swift":
        if random.random() < 0.5:
            measurements["app_start_cold"] = {"value": random.randint(500, 5000), "unit": "millisecond"}
        else:
            measurements["app_start_warm"] = {"value": random.randint(100, 2000), "unit": "millisecond"}
        measurements["frames_total"] = {"value": random.randint(100, 1000), "unit": "none"}
        measurements["frames_slow"] = {"value": random.randint(0, 50), "unit": "none"}
        measurements["frames_frozen"] = {"value": random.randint(0, 10), "unit": "none"}

    user = make_user()

    txn = {
        "event_id": event_id,
        "type": "transaction",
        "timestamp": start + duration_ms / 1000,
        "start_timestamp": start,
        "transaction": txn_name,
        "transaction_info": {"source": source},
        "platform": platform,
        "release": f"{project['name']}@{random.choice(RELEASES)}",
        "environment": random.choice(ENVIRONMENTS),
        "sdk": make_sdk(platform),
        "contexts": {
            **make_contexts(platform),
            "trace": {
                "trace_id": trace_id,
                "span_id": root_span_id,
                "op": op,
                "status": random.choice(["ok", "ok", "ok", "internal_error", "deadline_exceeded"]),
            },
        },
        "measurements": measurements,
        "spans": spans,
        "tags": make_tags(platform, project, True, txn_name),
        "user": user,
    }

    request_ctx = make_request_context(platform, project["name"])
    if request_ctx:
        txn["request"] = request_ctx

    server_name = random.choice(SERVER_NAMES)
    if server_name:
        txn["server_name"] = server_name

    return event_id, txn, trace_id, span_ids


def make_session(project):
    ts = rand_timestamp()
    started = time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime(ts))
    duration = round(random.uniform(0.5, 3600.0), 2)
    errors = random.choice([0, 0, 0, 0, 1, 1, 2, 3])
    status = random.choice(["ok", "ok", "ok", "exited", "crashed", "abnormal"])
    ip = f"{random.randint(1,223)}.{random.randint(0,255)}.{random.randint(0,255)}.{random.randint(1,254)}"

    return {
        "sid": str(uuid.uuid4()),
        "did": f"user{random.randint(1, 500)}",
        "seq": random.randint(0, 100),
        "init": random.choice([True, True, True, False]),
        "started": started,
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime(ts + duration)),
        "duration": duration,
        "status": status,
        "errors": errors,
        "attrs": {
            "release": f"{project['name']}@{random.choice(RELEASES)}",
            "environment": random.choice(ENVIRONMENTS),
            "ip_address": ip,
            "user_agent": random.choice(USER_AGENTS),
        },
    }


def make_sessions(project):
    ts = time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime(rand_timestamp()))
    exited = random.randint(50, 500)
    errored = random.randint(0, max(1, exited // 10))
    crashed = random.randint(0, max(1, errored // 3))
    return {
        "aggregates": [
            {"started": ts, "exited": exited, "errored": errored, "crashed": crashed}
        ],
        "attrs": {
            "release": f"{project['name']}@{random.choice(RELEASES)}",
            "environment": random.choice(ENVIRONMENTS),
        },
    }


def make_csp_report(project):
    return {
        "csp-report": {
            "document-uri": random.choice(CSP_URLS),
            "violated-directive": random.choice(CSP_DIRECTIVES),
            "blocked-uri": random.choice(CSP_BLOCKED_URIS),
            "original-policy": "default-src 'self'; script-src 'self' https://cdn.example.com",
            "disposition": "enforce",
            "referrer": "",
            "status-code": random.choice([200, 0]),
        }
    }


def make_check_in(project):
    slug, schedule = random.choice(MONITOR_SLUGS)
    status = random.choices(["ok", "ok", "ok", "error", "in_progress"], weights=[50, 20, 10, 15, 5])[0]
    duration = round(random.uniform(0.5, 120.0), 2) if status != "in_progress" else None

    check_in = {
        "check_in_id": str(uuid.uuid4()),
        "monitor_slug": slug,
        "status": status,
        "environment": random.choice(ENVIRONMENTS),
        "monitor_config": {
            "schedule": {"type": "crontab", "value": schedule},
            "checkin_margin": random.choice([5, 10, 15]),
            "max_runtime": random.choice([30, 60, 120, 300]),
            "timezone": "UTC",
        },
    }
    if duration is not None:
        check_in["duration"] = duration
    return check_in


def make_user_report(event_id):
    name = random.choice(USER_NAMES)
    first = name.split()[0].lower()
    return {
        "event_id": event_id,
        "name": name,
        "email": f"{first}{random.randint(1, 99)}@example.com",
        "comments": random.choice(USER_REPORT_COMMENTS),
    }


def make_client_report():
    reasons = ["sample_rate", "queue_overflow", "rate_limit", "network_error", "before_send"]
    categories = ["error", "transaction", "session", "attachment"]
    num = random.randint(1, 4)
    discarded = []
    for _ in range(num):
        discarded.append({
            "reason": random.choice(reasons),
            "category": random.choice(categories),
            "quantity": random.randint(1, 500),
        })
    return {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime(rand_timestamp())),
        "discarded_events": discarded,
    }


def make_attachment():
    snippet = random.choice(LOG_SNIPPETS)
    filename = random.choice(["crash.log", "application.log", "worker.log", "error-dump.txt"])
    return filename, snippet.encode("utf-8")


# ---------------------------------------------------------------------------
# Issue grouping: pick recurring errors per project
# ---------------------------------------------------------------------------

def build_recurring_errors(projects):
    """For each project, pick 3-5 error patterns that repeat 5-20 times."""
    recurring = {}
    for proj in projects:
        platform = proj["platform"]
        catalog = ERROR_CATALOG[platform]
        num_recurring = random.randint(3, min(5, len(catalog)))
        chosen = random.sample(catalog, num_recurring)
        recurring[proj["id"]] = [(p, random.randint(5, 20)) for p in chosen]
    return recurring


# ---------------------------------------------------------------------------
# Envelope header builder (adds sent_at and sdk)
# ---------------------------------------------------------------------------

def make_envelope_header(event_id, key, pid, platform):
    header = {"dsn": f"https://{key}@localhost/{pid}", "sent_at": iso_now()}
    if event_id:
        header["event_id"] = event_id
    sdk_name, sdk_version = SDK_BY_PLATFORM.get(platform, ("sentry.unknown", "0.0.0"))
    header["sdk"] = {"name": sdk_name, "version": sdk_version}
    return header


# ---------------------------------------------------------------------------
# Thread-safe stats counter
# ---------------------------------------------------------------------------

class Stats:
    def __init__(self):
        self._lock = threading.Lock()
        self.ok = 0
        self.err = 0
        self.sent = 0

    def record(self, status):
        with self._lock:
            if 200 <= status < 300:
                self.ok += 1
            else:
                self.err += 1
            self.sent += 1


# ---------------------------------------------------------------------------
# Generate a single work item (returns a callable that sends it)
# ---------------------------------------------------------------------------

def generate_item(project, kind, base, recurring_pattern=None, sent_error_ids=None):
    """Generate one item and return (kind, event_id_label, send_fn).

    send_fn() performs the HTTP request and returns (status, resp).
    This separation lets us pre-generate payloads, then send in parallel.
    """
    pid = project["id"]
    key = project["key"]
    platform = project["platform"]
    extra_sends = []  # additional sends for trace groups

    if kind == "error":
        event_id, event = make_error_event(project, error_pattern=recurring_pattern)
        payload = json.dumps(event, separators=(",", ":"))
        envelope = build_envelope(
            make_envelope_header(event_id, key, pid, platform),
            [({"type": "event"}, payload)],
        )
        url = f"{base}/api/{pid}/envelope/"
        if sent_error_ids is not None:
            sent_error_ids.append((event_id, project))

        def do_send():
            return send(url, envelope, key)
        return kind, event_id, do_send, extra_sends

    elif kind == "message":
        event_id, event = make_message_event(project)
        payload = json.dumps(event, separators=(",", ":"))
        url = f"{base}/api/{pid}/store/"

        def do_send():
            return send(url, payload, key, content_type="application/json")
        return kind, event_id, do_send, extra_sends

    elif kind == "transaction":
        event_id, txn, trace_id, span_ids = make_transaction(project)
        payload = json.dumps(txn, separators=(",", ":"))
        envelope = build_envelope(
            make_envelope_header(event_id, key, pid, platform),
            [({"type": "transaction"}, payload)],
        )
        url = f"{base}/api/{pid}/envelope/"

        # ~15% of transactions get a correlated error
        if random.random() < 0.15 and span_ids:
            err_id, err_event = make_error_event(project)
            err_event["contexts"]["trace"] = {
                "trace_id": trace_id,
                "span_id": random.choice(span_ids),
            }
            err_payload = json.dumps(err_event, separators=(",", ":"))
            err_envelope = build_envelope(
                make_envelope_header(err_id, key, pid, platform),
                [({"type": "event"}, err_payload)],
            )
            if sent_error_ids is not None:
                sent_error_ids.append((err_id, project))

            def send_correlated():
                return send(url, err_envelope, key)
            extra_sends.append(("error", err_id, send_correlated))

        def do_send():
            return send(url, envelope, key)
        return kind, event_id, do_send, extra_sends

    elif kind == "session":
        session = make_session(project)
        payload = json.dumps(session, separators=(",", ":"))
        envelope = build_envelope(
            make_envelope_header(None, key, pid, platform),
            [({"type": "session"}, payload)],
        )
        url = f"{base}/api/{pid}/envelope/"

        def do_send():
            return send(url, envelope, key)
        return kind, session["sid"][:8], do_send, extra_sends

    elif kind == "sessions":
        sessions = make_sessions(project)
        payload = json.dumps(sessions, separators=(",", ":"))
        envelope = build_envelope(
            make_envelope_header(None, key, pid, platform),
            [({"type": "sessions"}, payload)],
        )
        url = f"{base}/api/{pid}/envelope/"

        def do_send():
            return send(url, envelope, key)
        return kind, "sessions", do_send, extra_sends

    elif kind == "csp":
        report = make_csp_report(project)
        payload = json.dumps(report, separators=(",", ":"))
        url = f"{base}/api/{pid}/security/?sentry_key={key}"

        def do_send():
            return send(url, payload, key, content_type="application/csp-report")
        return kind, "csp", do_send, extra_sends

    elif kind == "check_in":
        check_in = make_check_in(project)
        payload = json.dumps(check_in, separators=(",", ":"))
        envelope = build_envelope(
            make_envelope_header(None, key, pid, platform),
            [({"type": "check_in"}, payload)],
        )
        url = f"{base}/api/{pid}/envelope/"

        def do_send():
            return send(url, envelope, key)
        return kind, check_in["check_in_id"][:8], do_send, extra_sends

    elif kind == "user_report":
        if sent_error_ids and len(sent_error_ids) > 0:
            ref_event_id, ref_project = random.choice(sent_error_ids)
            pid = ref_project["id"]
            key = ref_project["key"]
            platform = ref_project["platform"]
            report = make_user_report(ref_event_id)
        else:
            report = make_user_report(rand_event_id())
        payload = json.dumps(report, separators=(",", ":"))
        envelope = build_envelope(
            make_envelope_header(None, key, pid, platform),
            [({"type": "user_report"}, payload)],
        )
        url = f"{base}/api/{pid}/envelope/"

        def do_send():
            return send(url, envelope, key)
        return kind, "user_report", do_send, extra_sends

    elif kind == "client_report":
        report = make_client_report()
        payload = json.dumps(report, separators=(",", ":"))
        envelope = build_envelope(
            make_envelope_header(None, key, pid, platform),
            [({"type": "client_report"}, payload)],
        )
        url = f"{base}/api/{pid}/envelope/"

        def do_send():
            return send(url, envelope, key)
        return kind, "client_report", do_send, extra_sends

    elif kind == "attachment":
        err_id, err_event = make_error_event(project)
        err_payload = json.dumps(err_event, separators=(",", ":"))
        att_filename, att_data = make_attachment()
        envelope = build_envelope(
            make_envelope_header(err_id, key, pid, platform),
            [
                ({"type": "event"}, err_payload),
                ({"type": "attachment", "filename": att_filename, "content_type": "text/plain"}, att_data),
            ],
        )
        url = f"{base}/api/{pid}/envelope/"
        if sent_error_ids is not None:
            sent_error_ids.append((err_id, project))

        def do_send():
            return send(url, envelope, key)
        return kind, err_id, do_send, extra_sends

    # Fallback
    def noop():
        return 200, "noop"
    return kind, "unknown", noop, extra_sends


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Generate fake Sentry data for stackpit")
    parser.add_argument("--config", default="stackpit.toml", help="path to stackpit.toml")
    parser.add_argument("--base-url", default=None, help="override base URL (skips config discovery)")
    parser.add_argument("--count", type=int, default=100000, help="total number of items to generate")
    parser.add_argument("--seed", type=int, default=None, help="random seed for reproducibility")
    parser.add_argument("--workers", type=int, default=32, help="number of parallel workers")
    parser.add_argument("--batch-size", type=int, default=500, help="events to generate before submitting batch")
    parser.add_argument("--quiet", action="store_true", help="suppress per-event output, show progress only")
    args = parser.parse_args()

    if args.seed is not None:
        random.seed(args.seed)

    base = (args.base_url or discover_base_url(args.config)).rstrip("/")

    # Init connection pool
    _init_pool_manager(base)

    print(f"target: {base}")
    print(f"projects: {len(PROJECTS)}")
    print(f"generating: {args.count} items")
    print(f"workers: {args.workers}, batch_size: {args.batch_size}")
    if _HAS_URLLIB3:
        print("transport: urllib3 (connection pooling)")
    else:
        print("transport: urllib.request (no pooling)")
    print()

    stats = Stats()
    sent_error_ids = []

    # Pre-compute recurring error schedule
    recurring = build_recurring_errors(PROJECTS)
    recurring_queue = []
    for proj in PROJECTS:
        for pattern, count in recurring[proj["id"]]:
            for _ in range(count):
                recurring_queue.append((proj, pattern))
    random.shuffle(recurring_queue)

    # Event distribution weights
    weights = {
        "error": 40,
        "message": 10,
        "transaction": 25,
        "session": 5,
        "sessions": 3,
        "csp": 5,
        "check_in": 5,
        "user_report": 2,
        "client_report": 3,
        "attachment": 2,
    }
    kinds = list(weights.keys())
    kind_weights = [weights[k] for k in kinds]

    t_start = time.monotonic()

    # Generate all work items, then send in batches via thread pool
    work_items = []

    for i in range(args.count):
        use_recurring = len(recurring_queue) > 0 and random.random() < 0.35
        if use_recurring:
            project, pattern = recurring_queue.pop()
            item = generate_item(project, "error", base, recurring_pattern=pattern, sent_error_ids=sent_error_ids)
        else:
            project = random.choice(PROJECTS)
            kind = random.choices(kinds, weights=kind_weights)[0]
            item = generate_item(project, kind, base, sent_error_ids=sent_error_ids)

        work_items.append((i, project, item))

    # Drain remaining recurring errors
    for j, (project, pattern) in enumerate(recurring_queue):
        item = generate_item(project, "error", base, recurring_pattern=pattern, sent_error_ids=sent_error_ids)
        work_items.append((args.count + j, project, item))

    total = len(work_items)
    # Count extra sends (correlated errors)
    extra_count = sum(len(item[3]) for _, _, item in work_items)
    total_with_extras = total + extra_count

    print(f"generated {total} items ({extra_count} correlated extras), submitting...\n")

    def submit_one(idx, project, item):
        kind_label, eid_label, send_fn, extra = item
        status, resp = send_fn()
        stats.record(status)

        results = [(kind_label, eid_label, project, status)]

        for ex_kind, ex_eid, ex_fn in extra:
            s2, _ = ex_fn()
            stats.record(s2)
            results.append((ex_kind, ex_eid, project, s2))

        return idx, results

    completed = 0

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        # Submit in batches
        batch_start = 0
        while batch_start < len(work_items):
            batch_end = min(batch_start + args.batch_size, len(work_items))
            batch = work_items[batch_start:batch_end]

            futures = {
                executor.submit(submit_one, idx, project, item): idx
                for idx, project, item in batch
            }

            for future in as_completed(futures):
                idx, results = future.result()
                completed += len(results)

                if not args.quiet:
                    for k_label, e_label, proj, st in results:
                        ok = 200 <= st < 300
                        label = f"[{completed}/{total_with_extras}]"
                        proj_label = f"project={proj['id']} ({proj['name']})"
                        print(f"  {label} {k_label:<14} {proj_label:<45} -> {st} {'ok' if ok else 'FAIL'}")
                else:
                    sys.stdout.write(f"\r  progress: {completed}/{total_with_extras}")
                    sys.stdout.flush()

            batch_start = batch_end

    elapsed = time.monotonic() - t_start
    eps = stats.sent / elapsed if elapsed > 0 else 0

    if args.quiet:
        sys.stdout.write("\r" + " " * 60 + "\r")

    print(f"\nDone: {stats.ok} ok, {stats.err} failed ({stats.sent} total)")
    print(f"Time: {elapsed:.1f}s ({eps:.0f} events/sec)")


if __name__ == "__main__":
    main()
