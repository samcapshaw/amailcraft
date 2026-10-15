# CalDAV discovery

The desktop app can check an explicitly supplied CalDAV endpoint and list advertised calendars.
Open **More → View & connection → Find CalDAV calendars…**, enter an HTTPS endpoint, username
and app password, then choose **Find calendars**. The sheet shows calendar names and addresses
that can be copied. It works with the Apple styled and Omarchy shells; the command is also
available through persistent engine/MCP sessions.

This increment is discovery only. It does not add a saved calendar account, download events,
create local calendar folders, schedule refreshes, or write to the server. A successful result
proves discovery responded, not complete calendar compatibility. No live account was tested.

## Connection behavior

- Discovery uses read-only PROPFIND requests: current principal, calendar home and Depth 1 calendar listing.
- HTTPS and system certificate validation are required. There is no insecure UI override.
- Credentials remain in memory for the request and are not stored in settings, mailbox data or secret files.
  The password field clears when the lookup starts. Re-enter it to retry.
- Only one lookup can run per session. Closing the sheet does not start another request or cancel an in-flight request.
- Redirects and cross-origin principal/home/calendar links are rejected before credential forwarding.
  Providers requiring a different calendar host need its final endpoint entered explicitly. Automatic iCloud
  shard discovery is not implemented.
- Response bodies are capped at 1 MiB, XML parsing has depth/element limits, listings have at most
  256 calendars, and connection/read operations have timeouts. Only successful property blocks are used.
- Browser builds cannot open these connections.

## Command

`caldav.discover {}` opens setup. `{ "endpoint": "https://calendar.example.net/", "username": "person@example.net", "password": "…" }`
starts a desktop lookup and returns its job id. Jobs live only in that session; a one-shot CLI invocation cannot retain them for a later process. `{ "job": 1 }` polls the result. Use synthetic credentials
and local fixtures when developing; do not put real passwords into shell history or logs.

The implementation is original code based on [RFC 4791](https://www.rfc-editor.org/rfc/rfc4791)
and [RFC 5397](https://www.rfc-editor.org/rfc/rfc5397), using our bounded XML and HTTP clients.
No Omamail code was imported. See [the reuse assessment](reuse-omamail.md) for future candidates.

## Next increment

Persist opt-in calendar source metadata and credentials in the existing secret store, fetch a bounded
event window with calendar-query REPORT, and maintain separate read-only CalDAV snapshots. Resolve
recurrence/time-zone behavior and refresh failures with synthetic fixtures before enabling server writes.
