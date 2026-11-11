import { Injectable } from "@angular/core";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";

@Injectable({ providedIn: "root" })
export class NotificationPermissionService {
  async request(): Promise<boolean> {
    if (await isPermissionGranted()) {
      return true;
    }
    return (await requestPermission()) === "granted";
  }
}
