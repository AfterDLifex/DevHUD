# DevHUD --- Enhanced Product & Implementation Plan

> **Product name:** DevHUD\
> **Tagline:** Your development context, always within reach.\
> **Product description:** A local-first developer focus console with an
> always-on-top desktop HUD for sprint work, reminders, searchable work
> history, quick capture, and an encrypted credential vault.\
> **Desktop shell:** Tauri v2 · **Backend:** Rust · **Frontend:**
> React + TypeScript

------------------------------------------------------------------------

## 1. Product Vision

DevHUD is a lightweight desktop companion that keeps important
development context visible without forcing developers to leave their
IDE.

It brings together:

-   Current sprint goals, tasks, blockers, and production notes.
-   A quick-capture inbox for ideas and work discovered while coding.
-   Reminders and alarms that are noticeable but not disruptive.
-   Searchable sprint history and useful stand-up/retrospective
    summaries.
-   A local encrypted vault for development credentials and environment
    URLs.
-   A compact, keyboard-first overlay that can stay above the IDE.

**Core promise:** Capture quickly, remember reliably, retrieve
instantly, and stay in the development flow.

DevHUD is not intended to replace Jira, Azure DevOps, GitHub, Notion, or
a team's project-management system. It is a personal context layer that
helps a developer manage the information they need while working.

## 2. Product Principles

1.  **Non-intrusive by default:** No unexpected focus stealing, modal
    interruptions, or noisy animations.
2.  **Fast capture:** A task, note, blocker, or idea should take only a
    few seconds to record.
3.  **Local-first:** Core functionality works offline. No account or
    network connection is required.
4.  **Safe by design:** Credentials are encrypted, IPC commands are
    narrowly scoped, and backups are protected.
5.  **Recoverable:** Data should survive application crashes, upgrades,
    and accidental deletion.
6.  **Keyboard-friendly:** Common actions are available through
    shortcuts and a command palette.
7.  **Sprint-aware, not sprint-limited:** Information can belong to a
    sprint, a personal inbox, or a reusable reference collection.
8.  **Progressively enhanced:** Ship a dependable core before adding
    integrations and advanced automation.

## 3. Target Users and Main Workflows

### Primary user

A developer working across multiple tasks, environments, repositories,
and releases who needs quick access to personal work context.

### Key workflows

**Start the workday** 1. Open DevHUD automatically or with a global
shortcut. 2. See the active sprint goal, in-progress tasks, blockers,
and today's reminders. 3. Select the task currently being worked on.

**Capture something while coding** 1. Press the quick-capture shortcut.
2. Enter a short sentence. 3. Choose or accept the suggested type: task,
note, production reminder, blocker, or future idea. 4. Save without
switching away from the IDE.

**Prepare for a production release** 1. Open the production checklist.
2. Review critical environment-specific notes. 3. Mark each item as
verified. 4. Keep unresolved items visible until they are addressed.

**Prepare for stand-up** 1. Open the current sprint summary. 2. Review
completed, in-progress, and blocked work. 3. Copy a concise summary. 4.
Edit it before sharing with the team.

**Retrieve a credential** 1. Open the vault using its shortcut. 2.
Search by title, URL, username, or tag. 3. Unlock if necessary. 4. Copy
the requested field. 5. Let the clipboard clear safely after the
configured interval.

## 4. Feature Specification

### 4.1 Always-on-top HUD

The HUD is the defining feature. It should be useful in both compact and
expanded states.

**Window behavior** - Frameless, transparent, always-on-top window. -
Draggable and resizable, with saved size and position. - Remember
position per display where supported. - Start minimized, compact, or
expanded according to user preference. - Optional launch at
operating-system startup. - Hide/show through a global shortcut. -
Close-to-tray behavior with an explicit quit action. - Restore safely
after display resolution or monitor changes. - Configurable opacity and
compact/expanded layouts.

**Interaction modes** - **Interactive mode:** Normal mouse and keyboard
interaction. - **Click-through mode:** Mouse events pass through the HUD
to the application underneath. - **Temporary interaction mode:** A
shortcut temporarily restores HUD interaction without requiring the user
to hunt for a small control. - **Focus mode:** Shows only the selected
task, active blockers, and due alarms. - **Compact mode:** Shows a small
status bar with sprint name, task indicator, and urgent reminder count.

Do not assume click-through and interactive behavior will be identical
on Windows, macOS, and Linux. Build a small platform feasibility
prototype before committing to the final interaction design.

**HUD cards** - Current sprint and goal. - Current task. - Today's tasks
and due items. - Active blockers. - Production reminders. - Upcoming
alarms. - Quick capture. - Vault shortcut (never expose secrets directly
on the HUD).

**Important UX rules** - Do not steal IDE focus when refreshing data or
receiving an alarm. - Do not show sensitive vault values in HUD
previews. - Keep animations limited to meaningful state changes. - Allow
the user to pause, hide, or reposition the HUD immediately.

### 4.2 Sprint Workspace

**Sprint management** - Create, edit, activate, complete, and archive
sprints. - Store sprint name, goal, start date, end date, and status. -
Permit only one active sprint by default. - Warn before creating
overlapping active sprint periods. - Support a sprint without fixed
dates for personal workflows.

**Work item types** - Task - Production note - Future-work idea -
Blocker - General note

**Task fields** - Title and description. - Status: `todo`,
`in_progress`, `blocked`, `done`, `cancelled`. - Priority: `low`,
`normal`, `high`, `critical`. - Optional due date and reminder. -
Tags. - Created, updated, started, and completed timestamps. - Optional
source/reference URL.

**Production checklist** - Group items by environment, release, or
category. - Include a verification checkbox and optional verification
note. - Keep critical unresolved items prominent. - Preserve
checked/unchecked state in sprint archives. - Support reusable checklist
templates in a later milestone.

**Quick capture** - One-line capture from the HUD. - Keyboard shortcut
opens a small capture window. - Default destination is the current
sprint. - Allow capture into a global inbox when the correct sprint is
unknown. - Provide simple type prefixes, for example: -
`task: Fix refresh issue` - `blocker: Waiting for API access` -
`prod: Verify production callback URL` - `idea: Add offline export` -
`note: Retry logic is in tracking_service.rs` - Allow conversion between
item types without losing the original creation time. - Never silently
discard text when the user closes the capture window.

### 4.3 Personal Inbox and Knowledge Notes

This is a valuable addition because not every useful thought belongs to
the active sprint.

-   A global inbox for uncategorized captures.
-   Review and triage inbox items into a sprint, reference note, or
    backlog.
-   Pinned notes for frequently used development information.
-   Tags and optional categories.
-   Markdown support for code snippets, links, and formatted notes.
-   Full-text search across titles, descriptions, notes, and tags.
-   Optional links between a task and a note.
-   Archive or delete with a recovery period.

Keep this feature lightweight. DevHUD should not become a full document
editor.

### 4.4 Sprint History and Reporting

**Archive** - Archive a sprint with a consistent snapshot of its tasks,
notes, blockers, production checklist, and future-work items. - Preserve
original timestamps and final statuses. - Do not silently modify an
archived snapshot when the current sprint changes. - Require
confirmation before destructive archive operations.

**History browser** - Search by sprint name, date range, keyword, tag,
item type, and status. - Filter completed, incomplete, cancelled, and
carried-over work. - Open an archived sprint to inspect its snapshot. -
Show a timeline of sprint activity.

**Carry-over** - Select individual unfinished items to carry into the
active sprint. - Create a new item linked to its original item rather
than moving and destroying history. - Preserve the original sprint
reference. - Avoid duplicate carry-over when an item has already been
copied.

**Stand-up and retrospective summaries** - Generate summaries from
actual stored activity. - Sections: completed, in progress, blocked,
carried over, and upcoming. - Let users edit the generated text before
copying. - Support Markdown and plain-text copy. - Clearly distinguish
recorded facts from manually written reflections.

**Sprint comparison** - Compare planned versus completed work. - Show
counts by status and priority. - Show carry-over counts and unresolved
blockers. - Avoid claiming formal team velocity from personal task
counts unless the user configures an estimate field and understands the
limitations.

### 4.5 Reminders and Alarms

**Reminder types** - Task due-date reminder. - Custom one-time alarm. -
Repeating alarm. - Sprint-end review reminder. - Production checklist
reminder. - Follow-up reminder for a blocker.

**Behavior** - Local OS notification plus an in-HUD indicator. - Snooze:
5 minutes, 15 minutes, 1 hour, or custom. - Dismiss, complete, or open
the related item. - Repeat rules: daily, weekly, selected weekdays, or
custom interval. - Persist reminder state so app restarts do not lose
pending reminders. - Handle sleep, resume, timezone changes, and missed
alarms. - Prevent duplicate notifications when the app resumes. -
Respect quiet hours and Do Not Disturb settings. - Provide a
test-notification action.

**Scheduling design** - Store the intended trigger time and timezone
context. - Calculate the next occurrence in a dedicated Rust scheduling
service. - Treat snoozing as a separate occurrence state; do not
overwrite the original recurrence rule. - Use stable reminder IDs and
occurrence IDs to prevent duplicate delivery.

### 4.6 Encrypted Credential Vault

The vault is security-sensitive and should be implemented only after the
core productivity features are stable.

**Vault features** - Master-password unlock. - Search by title, URL,
username, and tag. - Credential types: login, API token, database
connection, SSH-related reference, and secure note. - Copy username,
password, or token. - Open the associated URL. - Password generator with
configurable length and character sets. - Auto-lock after inactivity. -
Lock on application exit and optionally on OS lock/suspend. - Clipboard
auto-clear after a configurable delay. - Import/export through an
explicitly protected workflow. - No credential contents in logs, crash
reports, analytics, or notification text.

**Cryptographic design** - Use Argon2id to derive a key-encryption key
from the master password and a unique random salt. - Use a vetted
AES-256-GCM implementation for authenticated encryption. - Generate a
fresh, cryptographically secure nonce for every encryption operation;
never reuse a nonce with the same key. - Store salt, nonce, KDF
parameters, ciphertext, and format version as required for decryption. -
Use authenticated additional data to bind encrypted records to their
record IDs and field purpose where appropriate. - Zeroize secret buffers
when practical and minimize the time secrets remain in memory. - Never
store the master password. - Version the vault format so future
cryptographic migrations are possible.

**Key management** - Do not store the raw master password or derived
vault key in SQLite. - Consider an optional OS-keychain-backed
convenience unlock only after the master-password flow is secure and
clearly explained. - A keychain convenience mode must not silently
weaken the master-password security model. - Document recovery
limitations: if the master password and all recovery material are lost,
encrypted data may be unrecoverable.

**Clipboard safety** - Clear the clipboard only if it still contains the
exact secret DevHUD copied. Do not erase newer content copied by another
application. - Warn that clipboard managers and OS-level clipboard
history may retain copied secrets. - Never log clipboard contents. -
Keep the auto-clear interval configurable.

**Tauri IPC restrictions** - Expose purpose-specific commands such as
`vault_unlock`, `vault_search`, `vault_copy_field`, and `vault_lock`. -
Never expose a generic decrypt-anything or arbitrary-file-read
command. - Validate record IDs and requested fields in Rust. - Keep
encryption and database access in the Rust backend. - Return only the
minimum data required by the UI. - Avoid returning passwords to React
when a backend-controlled clipboard operation can perform the copy.

### 4.7 Command Palette and Keyboard Navigation

Add after the basic HUD and capture flow work reliably.

-   `Ctrl/Cmd + K`: open command palette.
-   Search tasks, notes, sprints, and vault metadata.
-   Quick-add task, note, blocker, or idea.
-   Switch HUD mode.
-   Open reminders.
-   Navigate to current or archived sprint.
-   Lock the vault.
-   Show keyboard shortcut help.
-   Support arrow-key navigation, Enter to select, and Escape to close.
-   Never show decrypted vault secrets in command-palette search
    results.

### 4.8 Settings and Personalization

-   Theme: dark, light, and system.
-   HUD opacity, size, and default mode.
-   Per-monitor position where supported.
-   Global shortcut customization with conflict detection.
-   Startup behavior and tray behavior.
-   Reminder sound and notification preferences.
-   Quiet hours / Do Not Disturb schedule.
-   Default sprint and quick-capture destination.
-   Vault auto-lock and clipboard timeout.
-   Backup location and backup reminder.
-   Data export and restore.
-   Privacy and security information.

### 4.9 Backup, Restore, and Data Portability

This should be part of the MVP foundation, not an afterthought.

-   Automatic local database backup before schema migrations.
-   User-triggered backup.
-   Configurable backup retention.
-   Restore preview before replacing current data.
-   Export non-secret productivity data to JSON or Markdown.
-   Export vault data only through a separate encrypted export flow.
-   Validate backup integrity before restore.
-   Warn when a backup is older than the current database.
-   Never create an unencrypted backup containing vault plaintext.
-   Document where local application data is stored on each operating
    system.

### 4.10 Reliability and Privacy

-   Work fully offline for core functionality.
-   No telemetry by default.
-   No network calls for fonts, icons, or essential UI assets.
-   Structured logs with secret redaction.
-   Database transactions for multi-step changes.
-   SQLite foreign keys enabled.
-   Database migrations with version tracking.
-   Graceful handling of locked databases and disk-full errors.
-   Recover from invalid saved window positions.
-   Prevent duplicate scheduled jobs and notifications.
-   Provide a clear data directory and backup location in settings.

------------------------------------------------------------------------

## 5. Recommended Architecture

### 5.1 High-level structure

``` text
DevHUD
│
├── Tauri v2 Desktop Layer
│   ├── Main HUD window
│   ├── Quick Capture window
│   ├── Command Palette window
│   ├── System Tray
│   ├── Global Shortcuts
│   ├── Notifications
│   └── Window / Display Management
│
├── React + TypeScript UI
│   ├── HUD
│   ├── Sprint Workspace
│   ├── Inbox and Notes
│   ├── History and Reports
│   ├── Reminders
│   ├── Vault UI
│   └── Settings
│
├── Rust Application Services
│   ├── Sprint Service
│   ├── Work Item Service
│   ├── Notes and Search Service
│   ├── Archive and Reporting Service
│   ├── Reminder Scheduler
│   ├── Vault / Crypto Service
│   ├── Backup / Restore Service
│   └── Window State Service
│
└── Local Persistence
    ├── SQLite Database
    ├── SQLite FTS5 Search Index
    ├── Encrypted Vault Records
    ├── Database Backups
    └── Non-secret App Settings
```

### 5.2 Technology choices

  -----------------------------------------------------------------------
  Layer                   Recommended choice      Reason
  ----------------------- ----------------------- -----------------------
  Desktop shell           Tauri v2                Lightweight desktop
                                                  runtime and Rust
                                                  integration

  Backend                 Rust                    Secure local services
                                                  and predictable system
                                                  integration

  Frontend                React + TypeScript      Component ecosystem and
                                                  maintainable UI

  Styling                 Tailwind CSS            Consistent compact
                                                  layouts

  Components              shadcn/ui, selectively  Accessible building
                                                  blocks; avoid
                                                  unnecessary UI weight

  Database                SQLite                  Reliable local
                                                  persistence

  Rust database access    `rusqlite` or SQLx      Choose one approach and
                                                  use it consistently

  Search                  SQLite FTS5             Fast offline full-text
                                                  search

  Serialization           Serde                   Structured Rust data
                                                  and IPC payloads

  Time                    `time` or `chrono`      Consistent time
                                                  handling

  Password KDF            Argon2                  Master-password key
                                                  derivation

  Encryption              AES-GCM implementation  Authenticated
                          from a vetted Rust      encryption
                          crate                   

  Secret handling         `zeroize`               Reduce secret lifetime
                                                  in memory

  OS integration          Tauri v2 plugins and    Shortcuts,
                          narrowly scoped native  notifications,
                          code                    clipboard, opener

  Testing                 Rust unit/integration   Validate business logic
                          tests + frontend tests  and UI behavior
  -----------------------------------------------------------------------

**Dependency rule:** Use Tauri v2-compatible plugin versions. Verify
plugin maintenance, permissions, platform support, and security before
adopting them. Do not add a plugin just because it exists.

### 5.3 Suggested Rust module layout

``` text
src-tauri/src/
├── main.rs
├── lib.rs
├── commands/
│   ├── sprint_commands.rs
│   ├── work_item_commands.rs
│   ├── note_commands.rs
│   ├── history_commands.rs
│   ├── reminder_commands.rs
│   ├── vault_commands.rs
│   ├── backup_commands.rs
│   └── settings_commands.rs
├── models/
│   ├── sprint.rs
│   ├── work_item.rs
│   ├── note.rs
│   ├── reminder.rs
│   └── vault_item.rs
├── services/
│   ├── sprint_service.rs
│   ├── work_item_service.rs
│   ├── archive_service.rs
│   ├── search_service.rs
│   ├── reminder_service.rs
│   ├── vault_service.rs
│   └── backup_service.rs
├── database/
│   ├── connection.rs
│   ├── migrations.rs
│   └── repositories/
├── security/
│   ├── crypto.rs
│   ├── key_derivation.rs
│   └── secret_memory.rs
└── platform/
    ├── window_manager.rs
    ├── shortcuts.rs
    └── notifications.rs
```

Keep business rules in services, SQL in repositories, and UI-facing
validation in command handlers. Avoid putting all application logic in
`main.rs` or directly inside Tauri commands.

------------------------------------------------------------------------

## 6. Data Model

Use UUIDs for entity IDs, UTC timestamps for stored instants, and
explicit timezone information for user-configured schedules where
needed.

  -----------------------------------------------------------------------
  Entity                              Important fields
  ----------------------------------- -----------------------------------
  `Sprint`                            id, name, goal, start_at, end_at,
                                      status, created_at, archived_at

  `WorkItem`                          id, sprint_id, type, title,
                                      description, status, priority,
                                      due_at, completed_at, created_at,
                                      updated_at, source_item_id

  `Note`                              id, sprint_id, title, content,
                                      pinned, created_at, updated_at

  `Tag`                               id, name, color

  `WorkItemTag`                       work_item_id, tag_id

  `Reminder`                          id, work_item_id, title,
                                      trigger_at, timezone, repeat_rule,
                                      enabled

  `ReminderOccurrence`                id, reminder_id, scheduled_at,
                                      snoozed_until, delivered_at,
                                      dismissed_at, completed_at

  `SprintArchive`                     id, sprint_id, snapshot_json,
                                      archived_at, schema_version

  `ActivityEvent`                     id, entity_type, entity_id, action,
                                      occurred_at, safe_metadata

  `VaultItem`                         id, title, url, username_cipher,
                                      password_cipher, notes_cipher,
                                      tags_metadata, created_at,
                                      updated_at

  `VaultConfig`                       id, salt, kdf_parameters,
                                      format_version, verifier_cipher

  `AppSettings`                       key, value, updated_at
  -----------------------------------------------------------------------

### Data-model rules

-   Use foreign keys and appropriate indexes.
-   Enable SQLite foreign-key enforcement on every connection.
-   Use transactions for operations that update multiple related
    records.
-   Use soft deletion or a recovery area for important user-created
    items.
-   Keep archive snapshots independent from live records.
-   Store vault ciphertext separately from ordinary productivity content
    where practical.
-   Avoid putting secrets in `ActivityEvent`, search indexes, logs, or
    backup metadata.
-   Add database migrations from the first version.
-   Use an FTS5 index for searchable non-secret content only.
-   Define deletion behavior explicitly: cascade, restrict, or preserve
    an archive reference.

------------------------------------------------------------------------

## 7. Implementation Roadmap

Build in vertical slices: each milestone should produce a usable,
testable application.

### Phase 0 --- Technical feasibility and foundation

**Goal:** Prove the risky desktop behaviors before building the complete
UI.

Tasks: 1. Create the Tauri v2 + React + TypeScript project. 2. Create a
transparent frameless window. 3. Test always-on-top behavior. 4. Test
dragging, resizing, opacity, and position persistence. 5. Prototype
click-through and temporary interaction mode. 6. Test global shortcuts.
7. Test tray behavior and notifications. 8. Verify behavior on Windows,
macOS, and Linux. 9. Establish SQLite connection, migration runner, and
structured logging. 10. Set up automated formatting, linting, and tests.

**Exit criteria** - The HUD can be shown and hidden reliably. - It does
not steal focus during passive updates. - Click-through limitations are
documented per platform. - Window state survives restart. - Database
migrations run successfully.

### Phase 1 --- HUD + Current Sprint (MVP foundation)

**Goal:** Make DevHUD useful during everyday coding.

Tasks: 1. Build the compact HUD. 2. Build the expanded dashboard. 3. Add
current sprint creation and editing. 4. Add task CRUD and status
transitions. 5. Add production notes and future-work items. 6. Add
blockers and priority indicators. 7. Add quick capture. 8. Add global
inbox. 9. Add tags and basic filters. 10. Add keyboard shortcuts for
capture and HUD visibility.

**Exit criteria** - A user can create a sprint and manage its work
without leaving the HUD. - Quick capture is reliable and does not lose
text. - All data persists after restart. - Core workflows work offline.

### Phase 2 --- Reminders and notification reliability

**Goal:** Make reminders dependable without making the app noisy.

Tasks: 1. Add one-time reminders. 2. Add task due-date reminders. 3. Add
repeating reminders. 4. Add snooze, dismiss, and complete actions. 5.
Add OS notifications. 6. Add HUD reminder indicators. 7. Persist
reminder occurrences. 8. Handle sleep/resume and missed notifications.
9. Prevent duplicate delivery. 10. Add quiet hours and test-notification
settings.

**Exit criteria** - A reminder survives application restart. - Snooze
does not corrupt recurrence settings. - Missed reminders are handled
according to user preferences. - Notifications do not unexpectedly steal
focus.

### Phase 3 --- Sprint History and Reports

**Goal:** Preserve work context and make it easy to retrieve.

Tasks: 1. Add sprint archive workflow. 2. Snapshot sprint content. 3.
Build history browser. 4. Add FTS5 search. 5. Add date, tag, status, and
type filters. 6. Add carry-over with source references. 7. Generate
stand-up summaries. 8. Add retrospective summaries. 9. Add sprint
comparison metrics.

**Exit criteria** - Archived sprints remain stable. - Search finds
relevant historical records. - Carry-over does not destroy original
history. - Summaries accurately reflect stored data.

### Phase 4 --- Backup, Restore, and Hardening

**Goal:** Protect user data before introducing the vault.

Tasks: 1. Back up before migrations. 2. Add manual backup. 3. Add
configurable backup retention. 4. Add restore validation and preview. 5.
Add JSON/Markdown export for non-secret data. 6. Add error recovery for
database failures. 7. Test interrupted writes and application crashes.
8. Review logs for accidental sensitive-data exposure.

**Exit criteria** - A user can back up and restore productivity data. -
Invalid backups are rejected safely. - Database upgrades do not silently
destroy user data.

### Phase 5 --- Encrypted Vault

**Goal:** Deliver credential storage with a clearly defined security
model.

Tasks: 1. Write a vault threat model. 2. Implement Argon2id key
derivation with versioned parameters. 3. Implement AES-GCM authenticated
encryption. 4. Add random salt and nonce generation. 5. Add
master-password setup and unlock. 6. Add lock and auto-lock lifecycle.
7. Add encrypted credential CRUD. 8. Add metadata search without
indexing secret values. 9. Add backend-controlled clipboard copy. 10.
Add safe clipboard clearing. 11. Add password generator. 12. Add
encrypted export/import. 13. Add security tests and independent review
before recommending real production credentials.

**Exit criteria** - No plaintext credentials are persisted. - No generic
decrypt IPC command exists. - Nonces are never reused with the same
key. - Locking invalidates active vault access. - Clipboard cleanup does
not erase unrelated user content. - Backup and recovery behavior is
documented.

### Phase 6 --- Portfolio polish and release

**Goal:** Turn the application into a reliable, demonstrable portfolio
project.

Tasks: 1. Add onboarding and sample data. 2. Add empty states and
keyboard shortcut help. 3. Add accessibility and keyboard-navigation
review. 4. Add Windows, macOS, and Linux packaging. 5. Test startup,
upgrade, uninstall, and data preservation. 6. Add release notes and
screenshots. 7. Add architecture and security documentation. 8. Create a
demo scenario and short product video. 9. Add crash-safe diagnostics
with secret redaction. 10. Publish a clear roadmap and known platform
limitations.

------------------------------------------------------------------------

## 8. MVP Scope and Priorities

Do not attempt to build every feature in the first release.

  Priority   Feature                                         Release
  ---------- ----------------------------------------------- ---------
  P0         Transparent always-on-top HUD                   MVP
  P0         Drag, resize, opacity, compact/expanded modes   MVP
  P0         Show/hide shortcut                              MVP
  P0         Current sprint and task management              MVP
  P0         Production notes, future ideas, blockers        MVP
  P0         Quick capture and inbox                         MVP
  P0         One-time and due-date reminders                 MVP
  P0         Local SQLite persistence and migrations         MVP
  P0         Basic backup and restore                        MVP
  P1         Repeating reminders and quiet hours             Next
  P1         Sprint archive and search                       Next
  P1         Carry-over and stand-up summaries               Next
  P1         Tags and advanced filters                       Next
  P1         Command palette                                 Next
  P2         Encrypted vault                                 Later
  P2         Encrypted vault backup/import                   Later
  P2         Multi-monitor polish                            Later
  P2         Reusable production checklist templates         Later
  P2         Sprint comparisons and advanced reports         Later
  P3         Git repository context and integrations         Future
  P3         Optional synchronization                        Future
  P3         Configurable automation rules                   Future

**Important scope decision:** The vault is part of the overall product
vision, but it should not delay the first useful release.
Security-sensitive functionality deserves a separate milestone, test
plan, and threat model.

------------------------------------------------------------------------

## 9. UI and Screen Structure

### Main HUD

``` text
┌───────────────────────────────────────────────┐
│ DevHUD   Sprint 08 · 4 days left       _  □  × │
├───────────────────────────────────────────────┤
│ GOAL                                          │
│ Complete offline tracking reliability         │
├───────────────────────────────────────────────┤
│ CURRENT TASK                                  │
│ [In Progress] Fix reconnect handling          │
├───────────────────────────────────────────────┤
│ BLOCKERS                                      │
│ • Waiting for API deployment                  │
├───────────────────────────────────────────────┤
│ TODAY                                         │
│ □ Verify production callback URL              │
│ □ Review Linux notification behavior          │
├───────────────────────────────────────────────┤
│ QUICK CAPTURE                                 │
│ Add a task, note, blocker or idea...      +   │
├───────────────────────────────────────────────┤
│ 3 tasks open       1 reminder       Ctrl+K     │
└───────────────────────────────────────────────┘
```

### Suggested navigation

-   **HUD:** At-a-glance information.
-   **Sprint:** Current sprint and task management.
-   **Inbox:** Unsorted captures and ideas.
-   **History:** Archived sprints and reports.
-   **Reminders:** Upcoming, snoozed, and completed reminders.
-   **Vault:** Locked/unlocked credential manager.
-   **Settings:** Appearance, shortcuts, notifications, backups, and
    security.

Use a compact visual hierarchy, restrained color, readable typography,
and clear status indicators. Avoid turning the overlay into a miniature
full-screen project-management application.

------------------------------------------------------------------------

## 10. Non-Functional Requirements

  -----------------------------------------------------------------------
  Area                                Requirement
  ----------------------------------- -----------------------------------
  Startup                             Fast startup; no unnecessary
                                      network requests

  Idle usage                          Low CPU usage when no work is
                                      occurring

  Memory                              Avoid continuously polling the
                                      database or rebuilding the UI

  Persistence                         Transactional writes and crash-safe
                                      migrations

  Offline                             All MVP features work without
                                      internet

  Security                            Narrow IPC permissions and no
                                      secret logging

  Search                              Responsive local search over
                                      non-secret content

  Accessibility                       Keyboard navigation, focus
                                      visibility, adequate contrast

  Reliability                         Graceful handling of sleep/resume
                                      and display changes

  Portability                         Explicitly tested Windows, macOS,
                                      and Linux behavior

  Privacy                             No telemetry by default; clear data
                                      storage documentation
  -----------------------------------------------------------------------

------------------------------------------------------------------------

## 11. Testing Strategy

### Rust tests

-   Sprint creation and state transitions.
-   Work-item validation.
-   Archive snapshot consistency.
-   Carry-over relationships.
-   Reminder recurrence and snooze calculations.
-   Timezone and daylight-saving edge cases.
-   Database transactions and migration rollback.
-   Search indexing and deletion.
-   Vault encryption/decryption and authentication failures.
-   Nonce uniqueness and malformed ciphertext handling.
-   Secret-buffer cleanup where testable.

### Frontend tests

-   Compact and expanded HUD.
-   Quick-capture validation.
-   Task status and priority updates.
-   Empty states and filters.
-   Keyboard navigation.
-   Reminder actions.
-   Vault locked/unlocked states.
-   No secret values rendered in locked or search-preview states.

### Desktop integration tests

-   Always-on-top behavior.
-   Focus preservation.
-   Click-through and temporary interaction.
-   Global shortcut conflicts.
-   Multiple displays and resolution changes.
-   Sleep/resume.
-   Startup and tray behavior.
-   OS notifications.
-   Clipboard auto-clear race conditions.
-   Upgrade and data migration.
-   Backup and restore.

### Release acceptance checklist

-   [ ] No data loss during normal restart.
-   [ ] No duplicate reminders after resume.
-   [ ] No unexpected focus stealing.
-   [ ] Click-through behavior documented for each supported OS.
-   [ ] Database backup created before migrations.
-   [ ] Restore tested using a real backup.
-   [ ] No secrets appear in logs or notifications.
-   [ ] Vault security tests pass before vault release.
-   [ ] Installer and upgrade paths tested on supported platforms.

------------------------------------------------------------------------

## 12. Risks and Decisions to Resolve Early

  -----------------------------------------------------------------------
  Risk                                Mitigation
  ----------------------------------- -----------------------------------
  Click-through differs across        Build a cross-platform prototype in
  operating systems                   Phase 0

  Transparent windows can affect      Keep the HUD visually simple and
  performance or rendering            profile on real hardware

  Overlay becomes distracting         Compact mode, focus mode, opacity,
                                      and quiet behavior

  Reminder scheduling becomes         Persist occurrences and reconcile
  unreliable after sleep              on resume

  Sprint archives become inconsistent Snapshot records transactionally

  Vault implementation creates a      Threat model, narrow IPC, security
  false sense of security             tests, and review

  Clipboard managers retain copied    Explain OS limitations and keep
  secrets                             clipboard exposure short

  Database corruption or accidental   Backups, restore validation, and
  deletion                            recovery design

  Too much scope delays delivery      Ship vertical slices and keep the
                                      vault separate
  -----------------------------------------------------------------------

### Decisions to make before implementation

1.  Which operating systems are first-class targets for the initial
    release?
2.  Should the HUD be interactive by default, or click-through by
    default?
3.  Should the first version support multiple active sprints, or one
    active sprint?
4.  Should production notes be sprint-specific, reusable templates, or
    both?
5.  What is the minimum supported backup and recovery experience?
6.  Should vault convenience unlock through the OS keychain be
    supported, or should the first vault release require the master
    password every time?

------------------------------------------------------------------------

## 13. Recommended Development Order

The most effective implementation order is:

1.  **Prove the overlay:** transparent window, always-on-top, focus
    preservation, click-through, and shortcuts.
2.  **Build the data foundation:** SQLite, migrations, repositories, and
    typed Rust commands.
3.  **Deliver the sprint workflow:** tasks, blockers, production notes,
    future ideas, and quick capture.
4.  **Make reminders dependable:** persisted scheduling and OS
    notifications.
5.  **Protect user data:** backups, restore, and crash recovery.
6.  **Add history:** archives, search, carry-over, and reports.
7.  **Build the vault:** separate threat model and security-focused
    implementation.
8.  **Polish and release:** cross-platform testing, documentation, and
    portfolio demo.

**First milestone definition of done:** DevHUD can stay visible above
the IDE, be hidden and restored with a shortcut, display the active
sprint, manage tasks and blockers, capture notes quickly, and persist
everything locally. It must not steal focus during passive updates.

## 14. Portfolio Presentation

### One-line description

DevHUD is a local-first developer productivity HUD built with Tauri v2,
Rust, React, TypeScript, and SQLite. It keeps sprint context, tasks,
reminders, searchable work history, and an encrypted credential vault
accessible through a lightweight desktop overlay.

### What it demonstrates

-   Rust desktop application architecture.
-   Tauri v2 window management and operating-system integration.
-   Cross-platform desktop UX.
-   SQLite schema design and migrations.
-   Offline-first data persistence.
-   Reliable scheduling and notifications.
-   Search and historical reporting.
-   Applied cryptography and secret handling.
-   Secure frontend-to-backend IPC.
-   Testing, packaging, and release engineering.

### Suggested repository structure

``` text
devhud/
├── src/                     # React + TypeScript frontend
├── src-tauri/               # Rust + Tauri backend
├── docs/
│   ├── PRODUCT_PLAN.md
│   ├── ARCHITECTURE.md
│   ├── DATABASE_SCHEMA.md
│   ├── SECURITY_THREAT_MODEL.md
│   ├── IMPLEMENTATION_ROADMAP.md
│   └── RELEASE_CHECKLIST.md
├── screenshots/
├── README.md
└── CHANGELOG.md
```

------------------------------------------------------------------------

## 15. Final Product Direction

**Build DevHUD as a developer's personal context layer, not another
project-management platform.**

Its strongest differentiator is the combination of: - An unobtrusive
always-on-top desktop HUD. - Extremely fast capture while coding. -
Sprint-aware context and reliable reminders. - Searchable personal work
history. - A carefully designed local encrypted vault.

The best first release is not the one with the most features. It is the
one that a developer can leave running every day because it is genuinely
useful, reliable, quiet, and trustworthy.
