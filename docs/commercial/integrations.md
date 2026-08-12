# Integrations

> Commercial feature - requires a license including the `integrations` capability. See [Commercial features](./index.md) for the licensing model.

Stackpit alerts you by email out of the box. A license adds the two things teams tend to want next: delivery into Slack and generic webhooks, and filing an issue straight into the tracker you already use.

## What's gated, and what isn't

| Integration kind | Unlicensed | Licensed |
|---|---|---|
| Email (Lettermint, Postmark, SendGrid, SMTP) | Yes | Yes |
| Slack | No | Yes |
| Webhook (generic, HMAC-signed) | No | Yes |
| GitHub / Forgejo / GitLab issue creation | No | Yes |

Email is deliberately outside the gate. An unlicensed install is a fully working error tracker that still tells you when something breaks: new issues, regressions, threshold rules, and digests all go out by mail. What a license buys is *where else* those alerts land, and the ability to turn an issue into a ticket without leaving Stackpit.

Source links are not an integration and are never gated. Stackpit still detects your forge from a project's repository URL and links stack frames to the right file and line, licensed or not.

## Prerequisites

- A commercial license that includes the `integrations` feature, activated at `/web/admin/license`.
- For Slack: an incoming-webhook URL from your Slack workspace.
- For webhooks: a reachable HTTPS endpoint. Stackpit refuses URLs that resolve to private or internal addresses, and pins the resolved address for the actual request.
- For trackers: the tracker's base URL, an API token with permission to open issues, and the target - `owner`/`repo` for GitHub and Forgejo, or the numeric project id for GitLab.

Storing a Slack token, webhook secret, or tracker token requires a master key (`STACKPIT_MASTER_KEY`), or Stackpit refuses to save it rather than writing the credential in plaintext. See [secret encryption](../operator-guide.md#secret-encryption).

## Setting one up

1. Activate a license carrying `integrations` at `/web/admin/license`.
2. Go to **Settings → Integrations** and add the integration. Without a license the Slack, webhook, and issue-tracker buttons are visibly locked and the page explains why.
3. Enable it per project under **Project settings → Integrations**, choosing which triggers it fires on (new issues, regressions, thresholds, digests) and any level or environment filter.

Alert routing, digests, and threshold rules work the same for every kind; the license controls which kinds you can configure, not how routing behaves.

Filing a tracker issue is a per-issue action rather than a notification channel: open an issue and use the tracker button, or ask the MCP `create_tracker_issue` tool. Either way the tracker is authenticated with the integration's own stored token - no credential you presented to Stackpit is ever forwarded.

## What happens when a license lapses

The gate distinguishes configuring an integration from delivering through one, because breaking a production alerting path on the day a renewal is forgotten is worse than the revenue it protects.

| | Active | Grace (30 days past expiry) | Locked (no license, or past grace) |
|---|---|---|---|
| Existing Slack/webhook alerts fire | Yes | Yes | No |
| Add, edit, or enable an integration | Yes | No | No |
| Send a test notification | Yes | No | No |
| File a tracker issue | Yes | No | No |
| Email alerts | Yes | Yes | Yes |

Nothing is deleted at any point. Integrations, their encrypted credentials, and their per-project routing all survive a lapse untouched and resume the moment you activate a renewal.

When delivery is refused, it's logged (`notify: skipping <name> (<kind>) — an active commercial license is required`) rather than dropped silently, so a lapsed license shows up in your logs instead of as alerts that mysteriously stopped arriving.

## Where the code lives

The Slack, webhook, and tracker implementations sit in `src/commercial/providers/`, which puts them under `LICENSE-COMMERCIAL` rather than the MIT core. Email stays in `src/providers/`. That split is the point: the licensed capability is the code itself, not a flag the core checks.
