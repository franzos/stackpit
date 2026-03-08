#!/usr/bin/env python3
"""End-to-end sourcemap test for Stackpit.

Simulates a realistic JS project lifecycle:
1. Auto-registers a project by sending an event
2. Creates a release via the API
3. Builds a synthetic artifact bundle (sourcemaps + debug IDs)
4. Uploads it via the chunk-upload / assemble API
5. Sends JS error events with debug_meta referencing the sourcemaps
6. Prints URLs so you can verify source context in the UI

Usage:
    pip install requests
    python main.py [--config ../../stackpit.toml]
"""

import argparse
import hashlib
import io
import json
import os
import re
import sys
import time
import uuid
import zipfile

import requests

# ── Source fixtures ──────────────────────────────────────────────────
#
# Two "original" JS source files that our fake app is built from.
# These get embedded in sourcesContent so the server can show context.

UTILS_JS = """\
function formatCurrency(amount) {
  if (typeof amount !== 'number') {
    throw new TypeError('Amount must be a number');
  }
  return '$' + amount.toFixed(2);
}

function validateEmail(email) {
  if (!email.includes('@')) {
    throw new Error('Invalid email address: ' + email);
  }
  return true;
}

module.exports = { formatCurrency, validateEmail };
"""

APP_JS = """\
import { formatCurrency, validateEmail } from './utils';

function processOrder(order) {
  validateEmail(order.email);
  const total = formatCurrency(order.total);
  console.log('Order processed:', total);
  return { success: true, total };
}

function handleCheckout(cart) {
  if (!cart || !cart.items || cart.items.length === 0) {
    throw new Error('Cart is empty');
  }
  const order = { email: cart.email, total: cart.total };
  return processOrder(order);
}

module.exports = { processOrder, handleCheckout };
"""

# "Minified" output — each original function compressed to one line.
# Line 1: formatCurrency  (from utils.js line 1)
# Line 2: validateEmail    (from utils.js line 8)
# Line 3: processOrder     (from app.js line 3)
# Line 4: handleCheckout   (from app.js line 10)
APP_MIN_JS = """\
function formatCurrency(n){if(typeof n!=="number")throw new TypeError("Amount must be a number");return"$"+n.toFixed(2)}
function validateEmail(e){if(!e.includes("@"))throw new Error("Invalid email address: "+e);return!0}
function processOrder(o){validateEmail(o.email);var t=formatCurrency(o.total);console.log("Order processed:",t);return{success:!0,total:t}}
function handleCheckout(c){if(!c||!c.items||c.items.length===0)throw new Error("Cart is empty");var o={email:c.email,total:c.total};return processOrder(o)}
"""

APP_URL = "http://localhost:8080/static/app.min.js"
RELEASE = "frontend@1.0.0"

# ── VLQ encoder ─────────────────────────────────────────────────────

B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"


def vlq_encode(value):
    """Encode a single integer as a base64-VLQ string."""
    vlq = ((-value) << 1 | 1) if value < 0 else (value << 1)
    result = []
    while True:
        digit = vlq & 0x1F
        vlq >>= 5
        if vlq > 0:
            digit |= 0x20
        result.append(B64[digit])
        if vlq == 0:
            break
    return "".join(result)


def encode_segment(*values):
    return "".join(vlq_encode(v) for v in values)


# ── Sourcemap generation ────────────────────────────────────────────


def generate_sourcemap(debug_id):
    """Build a valid Source Map v3 with debug ID."""
    # Mappings: each line of app.min.js maps to a function in the originals.
    # Format per segment: (genCol, srcIdx_delta, srcLine_delta, srcCol_delta, name_delta)
    # All values are relative to previous segment state.
    #
    # Line 1: col 0 -> utils.js:0:0, name "formatCurrency"
    # Line 2: col 0 -> utils.js:7:0  (srcLine delta +7), name "validateEmail"
    # Line 3: col 0 -> app.js:2:0    (srcIdx +1, srcLine delta -5), name "processOrder"
    # Line 4: col 0 -> app.js:9:0    (srcIdx +0, srcLine delta +7), name "handleCheckout"

    segments = [
        encode_segment(0, 0, 0, 0, 0),     # line 1: AAAAA
        encode_segment(0, 0, 7, 0, 1),      # line 2: AAOAC
        encode_segment(0, 1, -5, 0, 1),     # line 3: ACLAC
        encode_segment(0, 0, 7, 0, 1),      # line 4: AAOAC
    ]
    mappings = ";".join(segments)

    return {
        "version": 3,
        "file": "app.min.js",
        "sources": ["src/utils.js", "src/app.js"],
        "sourcesContent": [UTILS_JS, APP_JS],
        "names": [
            "formatCurrency",
            "validateEmail",
            "processOrder",
            "handleCheckout",
        ],
        "mappings": mappings,
        "debug_id": debug_id,
        "debugId": debug_id,
    }


# ── Artifact bundle ─────────────────────────────────────────────────


def build_artifact_bundle(debug_id):
    """Create an artifact bundle ZIP like sentry-cli would."""
    sourcemap = generate_sourcemap(debug_id)
    sourcemap_json = json.dumps(sourcemap).encode()

    manifest = {
        "files": {
            "app.min.js.map": {
                "url": "~/static/app.min.js.map",
                "type": "source_map",
                "headers": {
                    "debug-id": debug_id,
                },
            },
            "app.min.js": {
                "url": "~/static/app.min.js",
                "type": "minified_source",
                "headers": {
                    "debug-id": debug_id,
                    "Sourcemap": "app.min.js.map",
                },
            },
        }
    }

    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("manifest.json", json.dumps(manifest))
        zf.writestr("app.min.js.map", sourcemap_json)
        zf.writestr("app.min.js", APP_MIN_JS)

    return buf.getvalue()


# ── Error event payloads ────────────────────────────────────────────

ERROR_SCENARIOS = [
    {
        "type": "TypeError",
        "value": "Amount must be a number",
        "lineno": 1,
        "colno": 53,
        "function": "formatCurrency",
    },
    {
        "type": "Error",
        "value": "Invalid email address: not-an-email",
        "lineno": 2,
        "colno": 45,
        "function": "validateEmail",
    },
    {
        "type": "Error",
        "value": "Cart is empty",
        "lineno": 4,
        "colno": 67,
        "function": "handleCheckout",
    },
    {
        "type": "TypeError",
        "value": "Cannot read properties of undefined (reading 'email')",
        "lineno": 3,
        "colno": 30,
        "function": "processOrder",
    },
]


def make_error_event(scenario, debug_id):
    """Build a raw Sentry event payload for a JS error."""
    event_id = uuid.uuid4().hex
    return {
        "event_id": event_id,
        "timestamp": time.time(),
        "platform": "javascript",
        "level": "error",
        "release": RELEASE,
        "environment": "production",
        "sdk": {"name": "sentry.javascript.browser", "version": "8.0.0"},
        "tags": [
            ["browser", "Chrome 120"],
            ["os", "Windows 11"],
            ["url", "http://localhost:8080/checkout"],
        ],
        "user": {
            "id": "user-42",
            "email": "test@example.com",
            "username": "testuser",
            "ip_address": "192.168.1.100",
        },
        "contexts": {
            "browser": {"name": "Chrome", "version": "120.0"},
            "os": {"name": "Windows", "version": "11"},
        },
        "breadcrumbs": {
            "values": [
                {
                    "timestamp": time.time() - 5,
                    "category": "navigation",
                    "message": "navigated to /checkout",
                    "level": "info",
                },
                {
                    "timestamp": time.time() - 2,
                    "category": "ui.click",
                    "message": "clicked button#place-order",
                    "level": "info",
                },
                {
                    "timestamp": time.time() - 1,
                    "category": "http",
                    "message": "POST /api/orders -> 422",
                    "level": "warning",
                },
            ]
        },
        "exception": {
            "values": [
                {
                    "type": scenario["type"],
                    "value": scenario["value"],
                    "mechanism": {"type": "onerror", "handled": False},
                    "stacktrace": {
                        "frames": [
                            # Bottom of stack — some generic entry point
                            {
                                "filename": APP_URL,
                                "abs_path": APP_URL,
                                "function": "HTMLButtonElement.onclick",
                                "lineno": 4,
                                "colno": 1,
                                "in_app": True,
                            },
                            # The actual error location
                            {
                                "filename": APP_URL,
                                "abs_path": APP_URL,
                                "function": scenario["function"],
                                "lineno": scenario["lineno"],
                                "colno": scenario["colno"],
                                "in_app": True,
                            },
                        ]
                    },
                }
            ]
        },
        "debug_meta": {
            "images": [
                {
                    "type": "sourcemap",
                    "debug_id": debug_id,
                    "code_file": APP_URL,
                }
            ]
        },
    }


# ── Config / discovery ──────────────────────────────────────────────


def read_config(config_path):
    """Parse the essentials from stackpit.toml."""
    cfg = {
        "ingest_bind": "0.0.0.0:3001",
        "bind": "0.0.0.0:3333",
        "admin_token": None,
    }

    if not os.path.isfile(config_path):
        return cfg

    with open(config_path) as f:
        for line in f:
            line = line.strip()
            for key in ("ingest_bind", "bind", "admin_token"):
                m = re.match(rf'^{key}\s*=\s*"([^"]*)"', line)
                if m:
                    cfg[key] = m.group(1)

    return cfg


def make_url(bind, path):
    host, _, port = bind.rpartition(":")
    if host in ("0.0.0.0", "::", ""):
        host = "127.0.0.1"
    return f"http://{host}:{port}{path}"


# ── Main flow ───────────────────────────────────────────────────────


def main():
    parser = argparse.ArgumentParser(description="Sourcemap end-to-end test")
    parser.add_argument(
        "--config",
        default="../../stackpit.toml",
        help="path to stackpit.toml (default: ../../stackpit.toml)",
    )
    parser.add_argument("--project-id", type=int, default=1)
    parser.add_argument(
        "--key", default="a" * 32, help="sentry key (default: 32 'a's)"
    )
    args = parser.parse_args()

    cfg = read_config(args.config)
    project_id = args.project_id
    sentry_key = args.key

    ingest_base = make_url(cfg["ingest_bind"], "")
    admin_base = make_url(cfg["bind"], "")

    admin_headers = {}
    if cfg["admin_token"]:
        admin_headers["Authorization"] = f"Bearer {cfg['admin_token']}"

    sentry_auth = f"Sentry sentry_key={sentry_key}, sentry_version=7"

    debug_id = str(uuid.uuid4())

    print(f"config:     {args.config}")
    print(f"ingest:     {ingest_base}")
    print(f"admin:      {admin_base}")
    print(f"project:    {project_id}")
    print(f"debug_id:   {debug_id}")
    print(f"release:    {RELEASE}")
    print()

    # -- Step 1: bootstrap project by sending a seed event ----------------
    print("[1/5] bootstrapping project...")
    seed = {
        "event_id": uuid.uuid4().hex,
        "timestamp": time.time(),
        "platform": "javascript",
        "level": "info",
        "message": "sourcemap test: project bootstrap",
        "release": RELEASE,
    }
    r = requests.post(
        f"{ingest_base}/api/{project_id}/store/",
        json=seed,
        headers={"X-Sentry-Auth": sentry_auth},
        timeout=10,
    )
    if r.status_code < 300:
        print(f"  ok (status {r.status_code})")
    else:
        print(f"  warning: store returned {r.status_code}: {r.text}")

    time.sleep(0.5)

    # -- Step 2: create release -------------------------------------------
    print("[2/5] creating release...")
    r = requests.post(
        f"{admin_base}/api/0/organizations/default/releases/",
        json={"version": RELEASE, "projects": [str(project_id)]},
        headers=admin_headers,
        timeout=10,
    )
    if r.status_code < 300:
        print(f"  ok (status {r.status_code})")
    else:
        print(f"  warning: release create returned {r.status_code}: {r.text}")

    # -- Step 3: build artifact bundle ------------------------------------
    print("[3/5] building artifact bundle...")
    bundle_data = build_artifact_bundle(debug_id)
    print(f"  bundle size: {len(bundle_data)} bytes")

    # -- Step 4: upload via chunk-upload + assemble -----------------------
    print("[4/5] uploading sourcemaps...")

    # 4a: check chunk-upload config
    r = requests.get(
        f"{admin_base}/api/0/organizations/default/chunk-upload/",
        headers=admin_headers,
        timeout=10,
    )
    if r.status_code != 200:
        print(f"  error: chunk-upload config returned {r.status_code}: {r.text}")
        sys.exit(1)

    upload_config = r.json()
    chunk_size = upload_config.get("chunkSize", 8 * 1024 * 1024)
    print(f"  chunk size: {chunk_size}")

    # 4b: split into chunks and upload
    chunks = []
    for i in range(0, len(bundle_data), chunk_size):
        chunk = bundle_data[i : i + chunk_size]
        checksum = hashlib.sha1(chunk).hexdigest()
        chunks.append(checksum)

        r = requests.post(
            f"{admin_base}/api/0/organizations/default/chunk-upload/",
            files={"file": (checksum, chunk, "application/octet-stream")},
            headers=admin_headers,
            timeout=30,
        )
        if r.status_code >= 300:
            print(f"  error: chunk upload returned {r.status_code}: {r.text}")
            sys.exit(1)

    print(f"  uploaded {len(chunks)} chunk(s)")

    # 4c: assemble
    bundle_checksum = hashlib.sha1(bundle_data).hexdigest()
    r = requests.post(
        f"{admin_base}/api/0/organizations/default/artifactbundle/assemble/",
        json={
            "checksum": bundle_checksum,
            "chunks": chunks,
            "projects": [project_id],
        },
        headers=admin_headers,
        timeout=30,
    )
    if r.status_code < 300:
        print(f"  assemble ok: {r.json()}")
    else:
        print(f"  error: assemble returned {r.status_code}: {r.text}")
        sys.exit(1)

    # -- Step 5: send error events ----------------------------------------
    print("[5/5] sending error events...")
    event_ids = []
    for scenario in ERROR_SCENARIOS:
        event = make_error_event(scenario, debug_id)
        event_ids.append(event["event_id"])

        r = requests.post(
            f"{ingest_base}/api/{project_id}/store/",
            json=event,
            headers={"X-Sentry-Auth": sentry_auth},
            timeout=10,
        )
        status = "ok" if r.status_code < 300 else f"error {r.status_code}"
        print(f"  {scenario['type']}: {scenario['value'][:50]} -> {status}")

        time.sleep(0.2)

    # -- Done! ------------------------------------------------------------
    print()
    print("done! check these URLs for source context:")
    print()
    web_base = make_url(cfg["bind"], "")
    print(f"  project issues: {web_base}/web/projects/{project_id}/issues/")
    for eid in event_ids:
        print(f"  event: {web_base}/web/projects/{project_id}/events/{eid}/")
    print()
    print("each stack frame should show the original source code with")
    print("context lines from utils.js / app.js instead of the minified code.")


if __name__ == "__main__":
    main()
