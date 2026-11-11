import { Injectable } from "@angular/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

@Injectable({ providedIn: "root" })
export class WindowControlService {
  async minimize(): Promise<void> {
    await getCurrentWindow().minimize();
  }

  async close(): Promise<void> {
    await getCurrentWindow().close();
  }

  async isMaximized(): Promise<boolean> {
    return getCurrentWindow().isMaximized();
  }

  async toggleMaximize(): Promise<boolean> {
    const win = getCurrentWindow();
    if (await win.isMaximized()) {
      await win.unmaximize();
    } else {
      await win.maximize();
    }
    return win.isMaximized();
  }
}