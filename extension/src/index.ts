/**
 * Focus Sync — Sigma FM extension entry point.
 *
 * Lifecycle:
 * - `activate` runs on `onStartup` (every Sigma launch with this extension enabled).
 * - Spawns the `focus-sync-sidecar.exe` binary as a long-running background
 *   task (host tracks per extension; see
 *   sigma-file-manager/src-tauri/src/extensions/processes.rs).
 * - Subscribes to Sigma's current path changes and pushes them via HTTP to
 *   the sidecar, which translates them into Windows UI Automation writes
 *   against any foreground file dialog.
 *
 * Why a sidecar binary is required:
 * - Sigma FM extensions run in a sandboxed JS / webview context. They cannot
 *   open raw TCP sockets or call Windows COM APIs directly.
 * - Windows UI Automation needs COM STA + UIAutomationCore. Only a native
 *   Win32 process can do that. Hence the sidecar binary handles the actual
 *   HWND / ValuePattern / Invoke work; the extension orchestrates.
 *
 * Design notes:
 * - IPC is HTTP, not TCP, because Sigma FM extensions can declare `http` host
 *   permission in the manifest but cannot open raw TCP sockets from a web worker.
 * - The sidecar serves HTTP on `127.0.0.1:37421` (loopback only, no firewall prompt).
 * - `sigma.shell.runWithProgress` returns a cancel handle the host uses to
 *   terminate the sidecar on extension deactivation / Sigma exit.
 *
 * Source attribution:
 * - The sidecar Rust source lives under `../sidecar/` (MIT, with Apache-2.0 for
 *   `fg_bypass.rs` — ported from `inaku-Gyan/PathWrap` (MIT) and
 *   `QwenLM/qwen-code` (Apache-2.0). See `../sidecar/README.md`.
 *
 * Naming history:
 * - The sidecar was originally called `spike.exe` during an internal research
 *   spike phase. As of v0.2.0 it has been renamed to `focus-sync-sidecar.exe`
 *   for clarity — same code, cleaner name. (The 'spike' name itself came from
 *   'S'idecar 'P'rocess for 'I'ntegration 'K'nowledge 'E'xtension.)
 */

import type {
  ExtensionActivationContext,
  ExtensionModule,
} from '@sigma-file-manager/api';

const SIDECAR_BINARY_ID = 'focus-sync-sidecar';
const SIDECAR_HTTP = 'http://127.0.0.1:37421';
const SIDECAR_DEFAULT_PORT = 37421;

interface SidecarRuntime {
  taskId: string;
  cancel: () => Promise<void>;
  port: number;
}

let sidecar: SidecarRuntime | null = null;
let enabled = true;
let currentPath: string | null = null;
let pushTimer: number | null = null;

/**
 * Push Sigma's current path to the sidecar (HTTP POST /set_path).
 * Debounced 100ms to coalesce rapid navigation.
 */
function schedulePush(path: string): void {
  currentPath = path;
  if (pushTimer !== null) {
    clearTimeout(pushTimer);
  }
  pushTimer = setTimeout(() => {
    pushTimer = null;
    void pushNow(path);
  }, 100) as unknown as number;
}

async function pushNow(path: string): Promise<void> {
  if (!enabled || !sidecar) return;
  try {
    await sigma.http.request({
      url: `${SIDECAR_HTTP}/set_path`,
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ path }),
    });
  } catch (e) {
    // Sidecar down or unreachable — log silently. Next activate will respawn.
    console.warn('[focus-sync] push failed:', e);
  }
}

async function syncNow(): Promise<void> {
  const path = await sigma.context.getCurrentPath();
  if (!path) {
    sigma.ui.showNotification({
      title: 'Focus Sync',
      description: 'No current path in Sigma navigator.',
      type: 'warning',
    });
    return;
  }
  await pushNow(path);
  sigma.ui.showNotification({
    title: 'Focus Sync',
    description: `Synced: ${path}`,
    type: 'success',
  });
}

async function toggle(): Promise<void> {
  enabled = !enabled;
  await sigma.storage.set('enabled', enabled);
  sigma.ui.showNotification({
    title: 'Focus Sync',
    description: enabled ? 'Enabled' : 'Disabled',
    type: 'info',
  });
}

export const activate: ExtensionModule['activate'] = async (
  ctx: ExtensionActivationContext
) => {
  // 1. Merge i18n strings.
  await sigma.i18n.mergeFromPath('locales');

  // 2. Restore persisted toggle state.
  enabled = (await sigma.storage.get<boolean>('enabled')) ?? true;

  // 3. Verify sidecar binary is installed (host manages downloads + integrity).
  const sidecarPath = await sigma.binary.getPath(SIDECAR_BINARY_ID);
  if (!sidecarPath) {
    sigma.ui.showNotification({
      title: 'Focus Sync',
      description:
        'Sidecar binary not installed. Open Extensions → Focus Sync → Install Binary.',
      type: 'error',
      duration: 8000,
    });
    return;
  }

  // 4. Spawn the sidecar as a long-running background task.
  //    `runWithProgress` returns a cancel handle the host uses on deactivate.
  try {
    const task = await sigma.shell.runWithProgress(
      sidecarPath,
      ['--ipc', 'http', '--port', String(SIDECAR_DEFAULT_PORT)],
      // onProgress: surface sidecar stderr to extension logs.
      ({ line, isStderr }) => {
        if (isStderr) {
          console.warn('[focus-sync][sidecar]', line);
        } else {
          console.log('[focus-sync][sidecar]', line);
        }
      }
    );
    sidecar = {
      taskId: task.taskId,
      cancel: task.cancel,
      port: SIDECAR_DEFAULT_PORT,
    };
  } catch (e) {
    sigma.ui.showNotification({
      title: 'Focus Sync',
      description: `Failed to start sidecar: ${String(e)}`,
      type: 'error',
    });
    return;
  }

  // 5. Register UI: commands + toolbar dropdown.
  sigma.commands.registerCommand(
    {
      id: 'focus-sync.syncNow',
      title: 'Focus Sync — Sync current path to all open dialogs',
    },
    syncNow
  );
  sigma.commands.registerCommand(
    {
      id: 'focus-sync.toggle',
      title: 'Focus Sync — Enable / Disable',
    },
    toggle
  );
  sigma.toolbar.registerDropdown(
    {
      id: 'focus-sync.toolbar',
      title: 'Focus Sync',
      icon: 'mdi-target',
      items: [
        { id: 'syncNow', title: 'Sync Now', commandId: 'focus-sync.syncNow' },
        { id: 'separator', title: '', separator: true },
        { id: 'toggle', title: 'Enable / Disable', commandId: 'focus-sync.toggle' },
      ],
    },
    { syncNow, toggle }
  );

  // 6. Subscribe to current path changes (the core trigger).
  sigma.context.onPathChange((path) => {
    if (path) schedulePush(path);
  });

  // 7. Push the current path immediately on activation.
  const initialPath = await sigma.context.getCurrentPath();
  if (initialPath) {
    schedulePush(initialPath);
  }

  console.log(`[focus-sync] activated, sidecar task=${sidecar.taskId}`);
};

export const deactivate: ExtensionModule['deactivate'] = async () => {
  if (pushTimer !== null) {
    clearTimeout(pushTimer);
    pushTimer = null;
  }
  if (sidecar) {
    try {
      // Politely ask the sidecar to quit (graceful, exits after HTTP response).
      await sigma.http.request({
        url: `${SIDECAR_HTTP}/quit`,
        method: 'POST',
      });
    } catch {
      // sidecar may already be down — that's fine.
    }
    try {
      await sidecar.cancel();
    } catch {
      // host may have already terminated the process tree.
    }
    sidecar = null;
  }
  console.log('[focus-sync] deactivated');
};

// Module shape required by host (matches `ExtensionModule` in @sigma-file-manager/api).
export default { activate, deactivate };