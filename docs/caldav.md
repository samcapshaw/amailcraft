# Read-only CalDAV calendars

Open **More → View & connection → CalDAV calendars…** and enter your provider’s HTTPS endpoint,
username and app password. Choose **Find calendars**, select an event calendar, re-enter the app
password and choose **Add read-only calendar**. The first refresh starts automatically.

Saved calendars appear in Calendar alongside local calendars. Their cached events remain available
offline. Reopen the CalDAV sheet and choose **Refresh** to update a source, or use **Manage → Refresh
automatically** to schedule updates while the desktop app is running. Automatic refresh is opt-in; the
default interval is one hour. Refresh covers 90 days back through one year ahead. The server must support calendar-query
with expanded events. Unexpanded recurrence rules or unreadable resources fail the refresh and keep
the last good snapshot. No live provider account has been tested.

## Manage a connection

Choose **Manage** beside a saved calendar to enable automatic refresh, choose an interval, or enter a
new app password. **Save password and reconnect** saves it to the credential store and immediately
retries the connection. Blank passwords leave the existing credential unchanged; they cannot be saved.

**Disconnect… → Disconnect and keep events** removes the saved password and source connection.
Downloaded appointments stay in the same folder as an editable local calendar. No server events are
deleted. Connection settings and disconnection are journaled outside editing Undo; older editing
snapshots cannot restore a removed connection or roll back its credentials/schedule.

Each source shows whether it is updating, up to date or needs attention, with last successful sync and
last-attempt times. Errors preserve downloaded events. Automatic refresh respects Work Offline,
uses one download worker, checks due sources every 30 seconds, and retries failures at the configured
interval (15 minutes minimum). Missed refreshes resume when the app is running and online; there is
no background service while the app is closed.

## Connection behavior

- Discovery uses read-only PROPFIND; event downloads use calendar-query REPORT. No server writes.
- HTTPS and system certificate validation are required; redirects and cross-origin links are rejected.
- Discovery passwords are transient and clear from the form when the lookup starts.
- Adding a calendar explicitly saves its password in the profile’s existing credential store. The sheet
  identifies that store: OS keyring where configured, otherwise the profile’s fallback store. Calendar
  metadata and mailbox journals contain the source URL and username, never its password.
- Downloads run on a worker and continue when the sheet closes. Completed updates are journaled
  as sync, outside Undo. Changed or removed source bindings discard in-flight results.
- Calendar items cannot be edited locally. Successful refreshes replace their snapshot and preserve
  item IDs for matching UID/recurrence identities. Failures retain cached events and record an error.
- XML bodies are capped at 1 MiB with depth/element limits, discovery at 256 calendars and refresh at
  2,000 resources/events. Network operations have timeouts. Only successful property blocks are used.
- Browser builds cannot open these connections. Automatic iCloud shard discovery, server writes and
  complete provider compatibility are future work.

## Command

`caldav.discover {}` opens setup. `{ "endpoint": "https://calendar.example.net/", "username": "person@example.net", "password": "…" }`
starts a desktop lookup and returns its job id. Jobs live only in that session; a one-shot CLI invocation cannot retain them for a later process. `{ "job": 1 }` polls the result. Use synthetic credentials
and local fixtures when developing; do not put real passwords into shell history or logs.

`caldav.add {"url": "https://calendar.example.net/team/", "username": "person@example.net", "password": "…", "name": "Team"}` saves a source and returns its folder id. `caldav.refresh {"folder": id}` starts a download; `{"job": id}` polls and applies it. `caldav.sources {}` lists saved sources and their last successful update/error. Desktop UI polling continues after the sheet closes; persistent CLI/MCP callers should poll their job until done.

The implementation is original code based on [RFC 4791](https://www.rfc-editor.org/rfc/rfc4791)
and [RFC 5397](https://www.rfc-editor.org/rfc/rfc5397), using our bounded XML and HTTP clients.
No Omamail code was imported. See [the reuse assessment](reuse-omamail.md) for future candidates.

`caldav.configure {"folder": id, "password": "…", "automatic": true, "refreshMin": 60}` updates a connection (all fields except folder are optional). `caldav.disconnect {"folder": id}` removes its saved credential and keeps events locally. `caldav.refreshDue {}` starts the oldest due source when idle; desktop polling calls this automatically.

## Next increment

Validate provider-specific recurrence and time-zone behavior with synthetic fixtures before enabling
server writes and meeting invitations.
