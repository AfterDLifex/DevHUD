import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";

export type WorkItemKind = "task" | "production_note" | "idea" | "blocker";
export type WorkItemStatus =
  | "todo"
  | "in_progress"
  | "blocked"
  | "done"
  | "cancelled";
export type WorkItemPriority = "low" | "normal" | "high" | "critical";

export interface Sprint {
  id: string;
  name: string;
  goal: string;
  startAt: string;
  endAt: string;
  status: "active" | "archived";
  createdAt: string;
  archivedAt: string | null;
}

export interface WorkItem {
  id: string;
  sprintId: string | null;
  kind: WorkItemKind;
  title: string;
  description: string;
  status: WorkItemStatus;
  priority: WorkItemPriority;
  dueAt: string | null;
  createdAt: string;
  updatedAt: string;
  startedAt: string | null;
  completedAt: string | null;
  sourceItemId: string | null;
}

export interface NewWorkItem {
  sprintId: string | null;
  kind: WorkItemKind;
  title: string;
  description: string;
  priority: WorkItemPriority;
  dueAt: string | null;
}

export interface Reminder {
  id: string;
  workItemId: string | null;
  title: string;
  triggerAt: string;
  timezone: string;
  repeatRule: string | null;
  enabled: boolean;
}

export interface NewReminder {
  workItemId: string | null;
  title: string;
  triggerAt: string;
  timezone: string;
}

export interface ReminderOccurrence {
  occurrenceId: string;
  reminderId: string;
  workItemId: string | null;
  title: string;
  scheduledAt: string;
}

export interface ArchivedSprint {
  sprint: Sprint;
  workItems: WorkItem[];
  archivedAt: string;
}

export interface SprintSummary {
  sprint: Sprint;
  completed: string[];
  inProgress: string[];
  blocked: string[];
  remaining: string[];
  carriedOver: string[];
}

export interface BackupInfo {
  id: string;
  createdAt: string;
  sizeBytes: number;
}

export interface BackupPreview {
  backup: BackupInfo;
  sprints: number;
  workItems: number;
  reminders: number;
  integrityOk: boolean;
}

export interface RestoreResult {
  restoredFrom: string;
  safetyBackup: BackupInfo;
}

export interface NewVaultItem {
  kind: string;
  title: string;
  url: string;
  username: string;
  password: string;
  notes: string;
  tags: string[];
}

export interface VaultItem {
  id: string;
  kind: string;
  title: string;
  url: string;
  username: string;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

@Injectable({ providedIn: "root" })
export class TauriBridgeService {
  private call<T>(
    command: string,
    payload: Record<string, unknown> = {},
  ): Promise<T> {
    return invoke<T>(command, payload);
  }

  getSprints(): Promise<Sprint[]> {
    return this.call("get_sprints");
  }

  createSprint(input: {
    name: string;
    goal: string;
    startAt: string;
    endAt: string;
  }): Promise<Sprint> {
    return this.call("create_sprint", input);
  }

  archiveSprint(id: string): Promise<void> {
    return this.call("archive_sprint", { id });
  }

  getWorkItems(sprintId?: string | null): Promise<WorkItem[]> {
    return this.call("get_work_items", { sprintId: sprintId ?? null });
  }

  addWorkItem(item: NewWorkItem): Promise<WorkItem> {
    return this.call("add_work_item", { item });
  }

  setWorkItemStatus(id: string, status: WorkItemStatus): Promise<WorkItem> {
    return this.call("set_work_item_status", { id, status });
  }

  getReminders(): Promise<Reminder[]> {
    return this.call("get_reminders");
  }

  createReminder(reminder: NewReminder): Promise<Reminder> {
    return this.call("create_reminder", { reminder });
  }

  getDueReminders(): Promise<ReminderOccurrence[]> {
    return this.call("get_due_reminders");
  }

  snoozeReminder(occurrenceId: string, minutes: 5 | 15 | 60): Promise<void> {
    return this.call("snooze_reminder", { occurrenceId, minutes });
  }

  dismissReminder(occurrenceId: string): Promise<void> {
    return this.call("dismiss_reminder", { occurrenceId });
  }

  cancelReminder(id: string): Promise<void> {
    return this.call("cancel_reminder", { id });
  }

  searchSprintHistory(query: string): Promise<ArchivedSprint[]> {
    return this.call("search_sprint_history", { query });
  }

  getSprintSummary(sprintId: string): Promise<SprintSummary> {
    return this.call("get_sprint_summary", { sprintId });
  }

  carryOverWorkItem(sourceItemId: string): Promise<WorkItem> {
    return this.call("carry_over_work_item", { sourceItemId });
  }

  createBackup(): Promise<BackupInfo> {
    return this.call("create_backup");
  }

  listBackups(): Promise<BackupInfo[]> {
    return this.call("list_backups");
  }

  previewBackup(id: string): Promise<BackupPreview> {
    return this.call("preview_backup", { id });
  }

  restoreBackup(id: string): Promise<RestoreResult> {
    return this.call("restore_backup", { id });
  }

  exportProductivityData(): Promise<string> {
    return this.call("export_productivity_data");
  }

  vaultStatus(): Promise<boolean> {
    return this.call("vault_status");
  }

  setupVault(password: string): Promise<void> {
    return this.call("vault_setup", { password });
  }

  unlockVault(password: string): Promise<void> {
    return this.call("vault_unlock", { password });
  }

  lockVault(): Promise<void> {
    return this.call("vault_lock");
  }

  isVaultUnlocked(): Promise<boolean> {
    return this.call("vault_is_unlocked");
  }

  addVaultItem(item: NewVaultItem): Promise<string> {
    return this.call("add_vault_item", { item });
  }

  searchVaultItems(query: string): Promise<VaultItem[]> {
    return this.call("search_vault_items", { query });
  }

  deleteVaultItem(id: string): Promise<boolean> {
    return this.call("delete_vault_item", { id });
  }

  copyVaultField(id: string, field: "username" | "password" | "notes"): Promise<void> {
    return this.call("copy_vault_field", { id, field });
  }
}
