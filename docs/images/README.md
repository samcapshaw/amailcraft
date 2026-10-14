# App screenshots

The repository README uses the current desktop interface profiles, rendered directly from the application with
synthetic mail. Refresh them on a Mac with a working Metal adapter:

```sh
./scripts/screenshots-mac.sh
```

| Image | View |
|---|---|
| `mac-inbox.png` | All Inboxes, selected message, light appearance |
| `mac-dark.png` | All Inboxes, selected message, dark appearance, mailbox sidebar hidden |
| `mac-settings.png` | General settings with layout previews, light appearance |
| `mac-accounts.png` | Account manager, light appearance |
| `microsoft365-setup.png` | Microsoft 365 Graph account setup with synthetic address |
| `microsoft365-calendar.png` | Connected read-only calendar UI with a synthetic offline fixture |
| `microsoft365-contacts.png` | Connected read-only contacts UI with a synthetic offline fixture |
| `microsoft365-edit-event.png` | Server-backed event edit sheet (synthetic offline fixture) |
| `microsoft365-edit-contact.png` | Server-backed contact edit sheet (synthetic offline fixture) |
| `microsoft365-response.png` | Explicit attendee response sheet (synthetic offline fixture) |
| `microsoft365-pending.png` | Persistent proposal review (synthetic offline fixture) |
| `mac-calendar.png` | Calendar month view with persistent module navigation |
| `mac-compose.png` | Compose, visible sender and signature selector, compact formatting |
| `theme-omarchy.png` | Omarchy/Tokyo Night compact layout, dark appearance |
| `theme-outlook.png` | Outlook layout with the inherited classic ribbon |

Renders use 2× resolution and the built-in synthetic personal/shared/delegated demo. These are
offscreen UI renders, without operating-system window decorations. No real mailbox data is used.
Attribution and licensing are covered by `docs/images/*.png` in [ATTRIBUTION.md](../../ATTRIBUTION.md).

`hero.png`, `dark.png`, `classic-ribbon.png`, `calendar.png`, and `narrow.png` are inherited
upstream screenshots of the classic shell. They are retained as historical assets and are not
used to represent the current Mac app in the README.
