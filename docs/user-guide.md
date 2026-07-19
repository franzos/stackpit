# User Guide

For people using the Stackpit web UI to browse and triage errors. You don't need to install or configure anything; this covers finding your way around the interface. If you run the service, see the [operator guide](./operator-guide.md).

One thing to know up front: Stackpit's UI has no per-user roles. Everyone who signs in sees the same interface and the same buttons, and actions like resolving or deleting an issue affect what everyone else sees. There's no "read-only viewer" mode, so treat resolve, ignore, and delete as shared, not personal.

## Signing in

Open the site and you'll land on the login page. There are two ways in, depending on how the operator set it up:

- Sign in with SSO — if your organization uses single sign-on, click the button and you'll be sent to your identity provider and back.
- Admin token — otherwise, paste the shared admin token and click Sign in. It's a single shared token (from the server's `stackpit.toml`), not a personal username and password, so there's no registration or password reset.

Sign out from the Logout button in the sidebar.

## Finding your way around

The left sidebar lists your Projects, Organizations, Settings, and Defaults, plus a language switcher. Pick a project and the sidebar switches to that project's sections. Sections only appear once that kind of data has arrived, so a plain error-tracking project usually shows Issues and Settings, with Logs, Releases, Monitors and the rest showing up only when there's something to see.

## The projects dashboard

The dashboard lists your projects with their platforms, issue and event counts, a small breakdown bar (errors, transactions, sessions), the latest release, and first/last seen times. Search by name, narrow to a time window with the period dropdown (1h through 365d, or all time), and click a column header to sort. From here, "All events" and "All releases" jump to the cross-project views.

## Browsing issues

A project's home is its issue list: an events-over-time chart on top, then a table of issues with the error title and message, a level badge, how many events and users each has hit, and first/last seen times.

Narrow the list with the filters:

- Search text
- Status: Unresolved, Resolved, or Ignored
- Level: fatal, error, warning, info, debug
- Release (when releases exist)
- Time range

Sort by events, first seen, or last seen. If you arrived by clicking a tag value, a "Filtered by tag" chip shows at the top with an × to clear it.

## Reading an issue

Click an issue to open it. The top-right corner has the status actions: Resolve or Ignore an unresolved issue, or Re-open / Un-ignore one that's already been actioned.

The Details tab is where you diagnose:

- Exception and stacktrace, with per-frame source context and local variables, and a hint to upload source maps when frames are minified.
- Breadcrumbs leading up to the error.
- Tags, contexts, and any additional data.
- The Request that triggered it (method, URL, headers, query, body).
- Attachments, each with a Download button, and the Raw JSON of the event.

The right sidebar summarizes the issue (status, level, event and user counts, first/last seen with release, fingerprint), shows a sparkline, lists tag facets you can click to filter, and shows the affected user. The All events tab lists the individual occurrences; open one for the same detail on a single event, with newer/older navigation.

## Triaging

You can action issues one at a time from the detail view, or in bulk from the issue list: tick the checkboxes and use Resolve, Ignore, or Delete, or use the "Resolve/Ignore/Delete all N" buttons to act on the whole filtered set. Delete asks for confirmation and is permanent. Inside an issue, "Discard future events" tells Stackpit to drop new events with that fingerprint (reversible with Undo discard). Remember these are shared actions.

## Logs

If the project sends logs, the Logs page is a searchable, level-filtered table (timestamp, level, message, trace id, release). It's for browsing; there are no per-row actions.

## Releases and health

Releases lists versions with their issue and event counts and an adoption percentage; sort by adoption to see what's picking up. Open a release for session health: total, OK, crashed and errored sessions, and crash-free session and user percentages, followed by the issues seen in that release.

## Monitors

If cron monitors are in use, the Monitors page shows each monitor's slug, last status, last check-in, and check-in count. It's view-only in the UI.

## Your defaults

The Defaults page lets you set a default status, level, and time range that pre-fill the issue filters, plus your language. These are saved in your browser (per browser, not per account), so they follow the browser you set them in.

## Organizations

If you belong to more than one organization, the Organizations page lets you switch which one is active and open its member list. Creating organizations and managing members are administrative actions.

## What the UI doesn't do

To set expectations: there's no assigning issues to people, no comments, and no personal profile or notification settings. Issue actions are shared across everyone signed in, and defaults live in your browser rather than an account.
