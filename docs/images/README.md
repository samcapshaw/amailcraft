# App screenshots

The repository README uses the current Mac interface, rendered directly from the application with
synthetic mail. Refresh them on a Mac with a working Metal adapter:

```sh
./scripts/screenshots-mac.sh
```

| Image | View |
|---|---|
| `mac-inbox.png` | All Inboxes, selected message, light appearance |
| `mac-dark.png` | All Inboxes, selected message, dark appearance |
| `mac-settings.png` | General settings, light appearance |
| `mac-accounts.png` | Account manager, light appearance |
| `mac-compose.png` | Compose, visible sender, compact actions |

Renders use 2× resolution and the built-in synthetic personal/shared/delegated demo. These are
offscreen UI renders, without operating-system window decorations. No real mailbox data is used.
Attribution and licensing are covered by `docs/images/*.png` in [ATTRIBUTION.md](../../ATTRIBUTION.md).

`hero.png`, `dark.png`, `classic-ribbon.png`, `calendar.png`, and `narrow.png` are inherited
upstream screenshots of the classic shell. They are retained as historical assets and are not
used to represent the current Mac app in the README.
