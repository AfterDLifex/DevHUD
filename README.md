# DevHUD

DevHUD is a local-first developer HUD built with Angular, Tauri v2, Rust, and SQLite. It keeps sprint goals, work items, blockers, quick captures, and due dates in a compact desktop workspace.

## Run locally

Install the JavaScript dependencies, then run the desktop application:

```bash
npm install
npm run tauri dev
```

For a frontend production build:

```bash
npm run build
```

The Angular application is served on port `1420`; the production output is `dist/devhud/browser`, matching the Tauri configuration.

## Current functionality

- Create and archive one active sprint at a time.
- Create sprint work items, production notes, blockers, and ideas.
- Track item status and optional due dates.
- Capture unassigned items in the inbox.
- Store sprints, work items, reminders, and reminder occurrences in local SQLite.
- Deliver one-time reminders through desktop notifications while DevHUD is running; snooze and dismiss are persisted.
- Archive sprint snapshots, search history, carry unfinished work without moving the original, and generate stand-up summaries.
- Create verified SQLite backups, preview and restore them with a pre-restore safety copy, and export productivity data as JSON.
- Store credentials in a local AES-256-GCM vault with Argon2id-derived keys, decrypted-only-in-memory search, timed clipboard clearing, and a five-minute idle lock.
- Upgrade the versioned SQLite schema without recreating existing sprint data.
- Run as a frameless, transparent, resizable, always-on-top desktop window.

SQLite data and backups are stored in the operating system's Tauri application-data directory. DevHUD creates an automatic SQLite backup before upgrading an existing schema. Reminder delivery requires OS notification permission and is not guaranteed after DevHUD exits. Repeating reminders, system-tray lifecycle, global shortcuts, and click-through mode remain later implementation milestones or platform feasibility work.

Vault records are encrypted at rest with AES-256-GCM; record identifiers are authenticated as associated data, and the Argon2id salt and versioned parameters are stored alongside the encrypted verifier. The master password is not stored and cannot be recovered. Search decrypts records in the Rust backend only while unlocked; results expose display metadata but never passwords or notes. Clipboard clearing happens after 30 seconds only if the clipboard still contains the copied value, and other apps may preserve clipboard history.

SQLite backups are not encrypted. Vault payloads remain ciphertext, but backups expose database metadata, encrypted verifier material, and KDF parameters; protect backups as sensitive files. JSON export includes productivity data and reminder metadata, but never vault contents. Repeating reminders, system-tray lifecycle, global shortcuts, and click-through mode remain later implementation milestones or platform feasibility work.

See [DevHUD_Angular_Tauri_Product_and_Implementation_Plan.md](./DevHUD_Angular_Tauri_Product_and_Implementation_Plan.md) for product direction and the full roadmap.
