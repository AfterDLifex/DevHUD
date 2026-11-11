import { CommonModule } from "@angular/common";
import { Component, computed, OnDestroy, OnInit, signal } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { NotificationPermissionService } from "./services/notification-permission.service";
import {
  NewWorkItem,
  ArchivedSprint,
  BackupInfo,
  BackupPreview,
  Reminder,
  ReminderOccurrence,
  Sprint,
  SprintSummary,
  TauriBridgeService,
  RestoreResult,
  WorkItem,
  WorkItemKind,
  WorkItemStatus,
  NewVaultItem,
  VaultItem,
} from "./services/tauri-bridge.service";

@Component({
  selector: "app-root",
  imports: [CommonModule, FormsModule],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent implements OnInit, OnDestroy {
  readonly page = signal<
    | "overview"
    | "sprint"
    | "inbox"
    | "upcoming"
    | "reminders"
    | "history"
    | "vault"
    | "settings"
  >("overview");
  readonly sprints = signal<Sprint[]>([]);
  readonly workItems = signal<WorkItem[]>([]);
  readonly reminders = signal<Reminder[]>([]);
  readonly dueReminders = signal<ReminderOccurrence[]>([]);
  readonly history = signal<ArchivedSprint[]>([]);
  readonly historySummary = signal<SprintSummary | null>(null);
  readonly backups = signal<BackupInfo[]>([]);
  readonly backupPreview = signal<BackupPreview | null>(null);
  readonly vaultConfigured = signal(false);
  readonly vaultUnlocked = signal(false);
  readonly vaultItems = signal<VaultItem[]>([]);
  readonly vaultBusy = signal(false);
  readonly showVaultItemForm = signal(false);
  readonly loading = signal(true);
  readonly saving = signal(false);
  readonly errorMessage = signal("");
  readonly successMessage = signal("");
  readonly showSprintForm = signal(false);
  readonly showItemForm = signal(false);
  readonly showReminderForm = signal(false);
  readonly historyLoading = signal(false);
  readonly backupBusy = signal(false);
  readonly activeSprint = computed(
    () => this.sprints().find((sprint) => sprint.status === "active") ?? null,
  );
  readonly sprintItems = computed(() => {
    const sprintId = this.activeSprint()?.id;
    return sprintId
      ? this.workItems().filter((item) => item.sprintId === sprintId)
      : [];
  });
  readonly inboxItems = computed(() =>
    this.workItems().filter((item) => item.sprintId === null),
  );
  readonly openItems = computed(() =>
    this.sprintItems().filter(
      (item) => item.status !== "done" && item.status !== "cancelled",
    ),
  );
  readonly completedCount = computed(
    () => this.sprintItems().filter((item) => item.status === "done").length,
  );
  readonly blockerCount = computed(
    () =>
      this.sprintItems().filter(
        (item) => item.kind === "blocker" || item.status === "blocked",
      ).length,
  );
  readonly currentTask = computed(
    () => this.sprintItems().find((item) => item.status === "in_progress") ?? null,
  );
  readonly otherOpenItems = computed(() => {
    const currentTaskId = this.currentTask()?.id;
    return this.openItems().filter((item) => item.id !== currentTaskId);
  });
  readonly upcomingItems = computed(() =>
    this.workItems()
      .filter(
        (item) =>
          item.dueAt &&
          item.status !== "done" &&
          item.status !== "cancelled",
      )
      .sort((a, b) => (a.dueAt ?? "").localeCompare(b.dueAt ?? "")),
  );
  readonly activeReminders = computed(() =>
    this.reminders().filter((reminder) => reminder.enabled),
  );

  sprintName = "";
  sprintGoal = "";
  sprintStart = this.dateInput(new Date());
  sprintEnd = this.dateInput(new Date(Date.now() + 14 * 24 * 60 * 60 * 1000));
  itemTitle = "";
  itemDescription = "";
  itemKind: WorkItemKind = "task";
  itemPriority: NewWorkItem["priority"] = "normal";
  itemDueAt = "";
  captureText = "";
  captureKind: WorkItemKind = "task";
  reminderTitle = "";
  reminderTime = this.dateTimeInput(new Date(Date.now() + 60 * 60 * 1000));
  historyQuery = "";
  vaultPassword = "";
  vaultPasswordConfirm = "";
  vaultQuery = "";
  vaultKind = "login";
  vaultTitle = "";
  vaultUrl = "";
  vaultUsername = "";
  vaultSecret = "";
  vaultNotes = "";
  vaultTags = "";
  readonly todayLabel = new Intl.DateTimeFormat("en", {
    weekday: "long",
    month: "long",
    day: "numeric",
  }).format(new Date());
  readonly timeZoneName =
    Intl.DateTimeFormat().resolvedOptions().timeZone || "local time";
  private unlistenReminder: UnlistenFn | null = null;
  private unlistenReminderError: UnlistenFn | null = null;
  private vaultStatusTimer: number | null = null;

  constructor(
    private readonly bridge: TauriBridgeService,
    private readonly notificationPermission: NotificationPermissionService,
  ) {}

  ngOnInit(): void {
    void this.initialize();
  }

  ngOnDestroy(): void {
    this.unlistenReminder?.();
    this.unlistenReminderError?.();
    if (this.vaultStatusTimer !== null) {
      window.clearInterval(this.vaultStatusTimer);
    }
  }

  private async initialize(): Promise<void> {
    try {
      this.unlistenReminder = await listen<ReminderOccurrence>(
        "reminder-due",
        ({ payload }) => {
          this.dueReminders.update((items) => [
            payload,
            ...items.filter(
              (item) => item.occurrenceId !== payload.occurrenceId,
            ),
          ]);
          this.notify(`Reminder: ${payload.title}`);
          void this.refreshReminders();
        },
      );
      this.unlistenReminderError = await listen<string>(
        "reminder-delivery-error",
        ({ payload }) => this.errorMessage.set(payload),
      );
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
    await this.refresh();
    await this.refreshVaultState();
    this.vaultStatusTimer = window.setInterval(() => {
      void this.syncVaultSession();
    }, 30_000);
  }

  async refresh(): Promise<void> {
    this.loading.set(true);
    this.errorMessage.set("");
    try {
      const [sprints, workItems, reminders, dueReminders] = await Promise.all([
        this.bridge.getSprints(),
        this.bridge.getWorkItems(),
        this.bridge.getReminders(),
        this.bridge.getDueReminders(),
      ]);
      this.sprints.set(sprints);
      this.workItems.set(workItems);
      this.reminders.set(reminders);
      this.dueReminders.set(dueReminders);
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.loading.set(false);
    }
  }

  async createSprint(): Promise<void> {
    if (!this.sprintName.trim() || !this.sprintStart || !this.sprintEnd) {
      this.errorMessage.set("Add a sprint name and both dates to continue.");
      return;
    }
    this.saving.set(true);
    this.errorMessage.set("");
    try {
      const sprint = await this.bridge.createSprint({
        name: this.sprintName.trim(),
        goal: this.sprintGoal.trim(),
        startAt: this.sprintStart,
        endAt: this.sprintEnd,
      });
      this.sprints.update((sprints) => [sprint, ...sprints]);
      this.sprintName = "";
      this.sprintGoal = "";
      this.showSprintForm.set(false);
      this.page.set("sprint");
      this.notify("Sprint created.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.saving.set(false);
    }
  }

  async archiveActiveSprint(): Promise<void> {
    const sprint = this.activeSprint();
    if (!sprint || !window.confirm(`Archive "${sprint.name}"? Its work will remain in history.`)) {
      return;
    }
    this.saving.set(true);
    this.errorMessage.set("");
    try {
      await this.bridge.archiveSprint(sprint.id);
      await this.refresh();
      this.notify("Sprint archived.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.saving.set(false);
    }
  }

  async createWorkItem(inbox = false): Promise<void> {
    const title = inbox ? this.captureText : this.itemTitle;
    if (!title.trim()) {
      this.errorMessage.set("Add a short title before saving.");
      return;
    }
    this.saving.set(true);
    this.errorMessage.set("");
    const item: NewWorkItem = {
      sprintId: inbox ? null : this.activeSprint()?.id ?? null,
      kind: inbox ? this.captureKind : this.itemKind,
      title: title.trim(),
      description: inbox ? "" : this.itemDescription.trim(),
      priority: inbox ? "normal" : this.itemPriority,
      dueAt: inbox || !this.itemDueAt ? null : this.itemDueAt,
    };
    try {
      const created = await this.bridge.addWorkItem(item);
      this.workItems.update((items) => [created, ...items]);
      if (inbox) {
        this.captureText = "";
      } else {
        this.itemTitle = "";
        this.itemDescription = "";
        this.itemPriority = "normal";
        this.itemDueAt = "";
        this.showItemForm.set(false);
      }
      this.notify(inbox ? "Captured to your inbox." : "Work item added.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.saving.set(false);
    }
  }

  async changeStatus(item: WorkItem, status: WorkItemStatus): Promise<void> {
    this.errorMessage.set("");
    try {
      const updated = await this.bridge.setWorkItemStatus(item.id, status);
      this.workItems.update((items) =>
        items.map((current) => (current.id === updated.id ? updated : current)),
      );
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async createReminder(): Promise<void> {
    const title = this.reminderTitle.trim();
    const trigger = new Date(this.reminderTime);
    if (!title || !this.reminderTime || Number.isNaN(trigger.getTime())) {
      this.errorMessage.set("Add a reminder title and a valid time.");
      return;
    }
    this.saving.set(true);
    this.errorMessage.set("");
    try {
      const permitted = await this.notificationPermission.request();
      if (!permitted) {
        this.errorMessage.set(
          "Desktop notifications are disabled. Allow notifications in your operating-system settings, then try again.",
        );
        return;
      }
      const reminder = await this.bridge.createReminder({
        workItemId: null,
        title,
        triggerAt: trigger.toISOString(),
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
      });
      this.reminders.update((items) => [reminder, ...items]);
      this.reminderTitle = "";
      this.reminderTime = this.dateTimeInput(
        new Date(Date.now() + 60 * 60 * 1000),
      );
      this.showReminderForm.set(false);
      this.notify("Reminder scheduled.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.saving.set(false);
    }
  }

  async snoozeReminder(
    reminder: ReminderOccurrence,
    minutes: 5 | 15 | 60,
  ): Promise<void> {
    try {
      await this.bridge.snoozeReminder(reminder.occurrenceId, minutes);
      this.removeDueReminder(reminder.occurrenceId);
      await this.refreshReminders();
      this.notify(`Snoozed for ${minutes} minutes.`);
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async dismissReminder(reminder: ReminderOccurrence): Promise<void> {
    try {
      await this.bridge.dismissReminder(reminder.occurrenceId);
      this.removeDueReminder(reminder.occurrenceId);
      await this.refreshReminders();
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async cancelReminder(reminder: Reminder): Promise<void> {
    try {
      await this.bridge.cancelReminder(reminder.id);
      this.reminders.update((items) =>
        items.map((item) =>
          item.id === reminder.id ? { ...item, enabled: false } : item,
        ),
      );
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async searchHistory(): Promise<void> {
    this.historyLoading.set(true);
    this.errorMessage.set("");
    try {
      this.history.set(
        await this.bridge.searchSprintHistory(this.historyQuery.trim()),
      );
      this.historySummary.set(null);
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.historyLoading.set(false);
    }
  }

  async showSprintSummary(sprintId: string): Promise<void> {
    this.errorMessage.set("");
    try {
      this.historySummary.set(await this.bridge.getSprintSummary(sprintId));
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async carryOver(item: WorkItem): Promise<void> {
    this.errorMessage.set("");
    try {
      const carried = await this.bridge.carryOverWorkItem(item.id);
      this.workItems.update((items) => [carried, ...items]);
      this.notify("Work carried into the active sprint. Its archived record is unchanged.");
      await this.searchHistory();
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async copySprintSummary(summary: SprintSummary): Promise<void> {
    const sections: string[] = [`${summary.sprint.name} — ${summary.sprint.goal}`];
    if (summary.completed.length) sections.push(`Completed:\n${summary.completed.map((title) => `- ${title}`).join("\n")}`);
    if (summary.inProgress.length) sections.push(`In progress:\n${summary.inProgress.map((title) => `- ${title}`).join("\n")}`);
    if (summary.blocked.length) sections.push(`Blocked:\n${summary.blocked.map((title) => `- ${title}`).join("\n")}`);
    if (summary.remaining.length) sections.push(`Remaining:\n${summary.remaining.map((title) => `- ${title}`).join("\n")}`);
    if (summary.carriedOver.length) sections.push(`Carried over:\n${summary.carriedOver.map((title) => `- ${title}`).join("\n")}`);
    try {
      await navigator.clipboard.writeText(sections.join("\n\n"));
      this.notify("Sprint summary copied.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async refreshBackups(): Promise<void> {
    this.errorMessage.set("");
    try {
      this.backups.set(await this.bridge.listBackups());
      this.backupPreview.set(null);
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async createBackup(): Promise<void> {
    this.backupBusy.set(true);
    this.errorMessage.set("");
    try {
      const backup = await this.bridge.createBackup();
      this.backups.update((items) => [backup, ...items]);
      this.notify("Backup created.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.backupBusy.set(false);
    }
  }

  async previewBackup(backup: BackupInfo): Promise<void> {
    this.errorMessage.set("");
    try {
      this.backupPreview.set(await this.bridge.previewBackup(backup.id));
    } catch (error) {
      this.backupPreview.set(null);
      this.errorMessage.set(this.describeError(error));
    }
  }

  async restoreBackup(backup: BackupInfo): Promise<void> {
    const preview = this.backupPreview();
    if (!preview || preview.backup.id !== backup.id || !preview.integrityOk) {
      this.errorMessage.set("Preview and validate this backup before restoring.");
      return;
    }
    if (
      !window.confirm(
        `Replace current data with this backup (${preview.sprints} sprints, ${preview.workItems} work items)? A safety backup will be created first.`,
      )
    ) {
      return;
    }
    this.backupBusy.set(true);
    this.errorMessage.set("");
    try {
      const result: RestoreResult = await this.bridge.restoreBackup(backup.id);
      await this.refresh();
      await this.refreshBackups();
      this.notify(`Restored. Safety copy: ${result.safetyBackup.id}`);
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.backupBusy.set(false);
    }
  }

  async exportData(): Promise<void> {
    this.backupBusy.set(true);
    this.errorMessage.set("");
    try {
      const json = await this.bridge.exportProductivityData();
      const url = URL.createObjectURL(
        new Blob([json], { type: "application/json" }),
      );
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `devhud-export-${this.dateInput(new Date())}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
      this.notify("Productivity data exported.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.backupBusy.set(false);
    }
  }

  async submitVaultPassword(): Promise<void> {
    const alreadyConfigured = this.vaultConfigured();
    this.vaultBusy.set(true);
    this.errorMessage.set("");
    try {
      if (this.vaultConfigured()) {
        await this.bridge.unlockVault(this.vaultPassword);
      } else {
        if (this.vaultPassword !== this.vaultPasswordConfirm) {
          this.errorMessage.set("The master passwords do not match.");
          return;
        }
        await this.bridge.setupVault(this.vaultPassword);
        this.vaultConfigured.set(true);
      }
      this.vaultPassword = "";
      this.vaultPasswordConfirm = "";
      this.vaultUnlocked.set(true);
      await this.refreshVaultItems();
      this.notify(alreadyConfigured ? "Vault unlocked." : "Vault ready.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.vaultPassword = "";
      this.vaultPasswordConfirm = "";
      this.vaultBusy.set(false);
    }
  }

  async lockVault(): Promise<void> {
    try {
      await this.bridge.lockVault();
      this.vaultUnlocked.set(false);
      this.vaultItems.set([]);
      this.vaultQuery = "";
      this.vaultPassword = "";
      this.vaultSecret = "";
      this.showVaultItemForm.set(false);
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async refreshVaultItems(): Promise<void> {
    if (!this.vaultUnlocked()) return;
    this.errorMessage.set("");
    try {
      this.vaultItems.set(await this.bridge.searchVaultItems(this.vaultQuery));
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
      await this.syncVaultSession();
    }
  }

  async saveVaultItem(): Promise<void> {
    const title = this.vaultTitle.trim();
    if (!title) {
      this.errorMessage.set("Add a title for this vault item.");
      return;
    }
    this.vaultBusy.set(true);
    this.errorMessage.set("");
    const item: NewVaultItem = {
      kind: this.vaultKind.trim() || "login",
      title,
      url: this.vaultUrl.trim(),
      username: this.vaultUsername,
      password: this.vaultSecret,
      notes: this.vaultNotes,
      tags: this.vaultTags.split(",").map((tag) => tag.trim()).filter(Boolean),
    };
    try {
      await this.bridge.addVaultItem(item);
      this.vaultTitle = "";
      this.vaultUrl = "";
      this.vaultUsername = "";
      this.vaultSecret = "";
      this.vaultNotes = "";
      this.vaultTags = "";
      this.showVaultItemForm.set(false);
      await this.refreshVaultItems();
      this.notify("Encrypted vault item saved.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    } finally {
      this.vaultSecret = "";
      this.vaultBusy.set(false);
    }
  }

  async copyVaultField(item: VaultItem, field: "username" | "password" | "notes"): Promise<void> {
    try {
      await this.bridge.copyVaultField(item.id, field);
      this.notify(`${field === "password" ? "Password" : field === "username" ? "Username" : "Notes"} copied. Clipboard clears after 30 seconds if unchanged.`);
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async deleteVaultItem(item: VaultItem): Promise<void> {
    if (!window.confirm(`Permanently delete "${item.title}" from the vault?`)) return;
    try {
      await this.bridge.deleteVaultItem(item.id);
      this.vaultItems.update((items) => items.filter((current) => current.id !== item.id));
      this.notify("Vault item deleted.");
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  navigate(
    page:
      | "overview"
      | "sprint"
      | "inbox"
      | "upcoming"
      | "reminders"
      | "history"
      | "vault"
      | "settings",
  ): void {
    if (this.page() === "vault" && page !== "vault" && this.vaultUnlocked()) {
      void this.lockVault();
    }
    this.page.set(page);
    this.errorMessage.set("");
    if (page === "history") {
      void this.searchHistory();
    }
    if (page === "settings") {
      void this.refreshBackups();
    }
    if (page === "vault") {
      void this.refreshVaultState();
    }
  }

  private async refreshVaultState(): Promise<void> {
    try {
      this.vaultConfigured.set(await this.bridge.vaultStatus());
      await this.syncVaultSession();
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  private async syncVaultSession(): Promise<void> {
    try {
      const unlocked = await this.bridge.isVaultUnlocked();
      this.vaultUnlocked.set(unlocked);
      if (!unlocked) {
        this.vaultItems.set([]);
        this.vaultSecret = "";
        this.showVaultItemForm.set(false);
      }
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  backupSize(size: number): string {
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    return `${(size / (1024 * 1024)).toFixed(1)} MB`;
  }

  dueLabel(value: string | null): string {
    if (!value) {
      return "No due date";
    }
    const date = new Date(value.length === 10 ? `${value}T12:00:00` : value);
    if (Number.isNaN(date.getTime())) {
      return value;
    }
    return new Intl.DateTimeFormat("en", {
      month: "short",
      day: "numeric",
    }).format(date);
  }

  statusLabel(status: WorkItemStatus): string {
    return status.replace("_", " ");
  }

  async minimizeWindow(): Promise<void> {
    try {
      await getCurrentWindow().minimize();
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  async closeWindow(): Promise<void> {
    try {
      await getCurrentWindow().close();
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  private async refreshReminders(): Promise<void> {
    try {
      this.reminders.set(await this.bridge.getReminders());
    } catch (error) {
      this.errorMessage.set(this.describeError(error));
    }
  }

  private removeDueReminder(occurrenceId: string): void {
    this.dueReminders.update((items) =>
      items.filter((item) => item.occurrenceId !== occurrenceId),
    );
  }

  private notify(message: string): void {
    this.successMessage.set(message);
    window.setTimeout(() => this.successMessage.set(""), 2600);
  }

  private describeError(error: unknown): string {
    if (typeof error === "string") {
      return error;
    }
    if (error instanceof Error) {
      return error.message;
    }
    return "Something went wrong. Please try again.";
  }

  private dateInput(date: Date): string {
    const localDate = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
    return localDate.toISOString().slice(0, 10);
  }

  private dateTimeInput(date: Date): string {
    const localDate = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
    return localDate.toISOString().slice(0, 16);
  }
}
