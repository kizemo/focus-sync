/**
 * Focus Sync — Sigma FM extension entry point.
 *
 * Lifecycle:
 * - `activate` runs on `onStartup` (every Sigma launch with this extension enabled).
 * - Spawns `spike.exe` as a long-running background task (host tracks per extension).
 * - Subscribes to Sigma's current path changes and pushes them to spike.
 * - Spike translates paths to UIA dialog writes (Windows file dialogs).
 *
 * Design notes:
 * - IPC is HTTP, not TCP, because Sigma FM extensions can declare `http` host
 *   permission in the manifest but cannot open raw TCP sockets from a web worker.
 * - Spike serves HTTP on `127.0.0.1:37421` (loopback only, no firewall prompt).
 * - `sigma.shell.runWithProgress` returns a cancel handle the host uses to
 *   terminate spike on extension deactivation / Sigma exit
 *   (see `sigma-file-manager/src-tauri/src/extensions/processes.rs`).
 *
 * Source attribution:
 * - `spike.exe` Rust source under `../spike/` (MIT, with Apache-2.0 fg_bypass.rs)
 *   — ported from `inaku-Gyan/PathWrap` (MIT) + `QwenLM/qwen-code` (Apache-2.0)
 *   during spike phase. See `../spike/README.md`.
 */

import type {
  ExtensionActivationContext,
  ExtensionModule,
} from '@sigma-file-manager/api';

const SPIKE_BINARY_ID = 'spike';
const SPIKE_HTTP = 'http://127.0.0.1:37421';
const SPIKE_DEFAULT_PORT = 37421;

interface SpikeRuntime {
  taskId: string;
  cancel: () => Promise<void>;
  port: number;
}

let spike: SpikeRuntime | null = null;
let enabled = true;
let currentPath: string | null = null;
let pushTimer: number | null = null;

/**
 * Push Sigma's current path to spike (HTTP POST /set_path).
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
  if (!enabled || !spike) return;
  try {
    await sigma.http.request({
      url: `${SPIKE_HTTP}/set_path`,
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ path }),
    });
  } catch (e) {
    // spike down or unreachable — log silently. Next activate will respawn.
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

  // 3. Verify spike binary is installed (host manages downloads + integrity).
  const spikePath = await sigma.binary.getPath(SPIKE_BINARY_ID);
  if (!spikePath) {
    sigma.ui.showNotification({
      title: 'Focus Sync',
      description:
        'spike binary not installed. Open Extensions → Focus Sync → Install Binary.',
      type: 'error',
      duration: 8000,
    });
    return;
  }

  // 4. Spawn spike as long-running background task.
  //    `runWithProgress` returns a cancel handle the host uses on deactivate.
  try {
    const task = await sigma.shell.runWithProgress(
      spikePath,
      ['--ipc', 'http', '--port', String(SPIKE_DEFAULT_PORT)],
      // onProgress: surface spike stderr to extension logs.
      ({ line, isStderr }) => {
        if (isStderr) {
          console.warn('[focus-sync][spike]', line);
        } else {
          console.log('[focus-sync][spike]', line);
        }
      }
    );
    spike = {
      taskId: task.taskId,
      cancel: task.cancel,
      port: SPIKE_DEFAULT_PORT,
    };
  } catch (e) {
    sigma.ui.showNotification({
      title: 'Focus Sync',
      description: `Failed to start spike: ${String(e)}`,
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

  console.log(`[focus-sync] activated, spike pid task=${spike.taskId}`);
};

export const deactivate: ExtensionModule['deactivate'] = async () => {
  if (pushTimer !== null) {
    clearTimeout(pushTimer);
    pushTimer = null;
  }
  if (spike) {
    try {
      // Politely ask spike to quit (graceful, exits after HTTP response).
      await sigma.http.request({
        url: `${SPIKE_HTTP}/quit`,
        method: 'POST',
      });
    } catch {
      // spike may already be down — that's fine.
    }
    try {
      await spike.cancel();
    } catch {
      // host may have already terminated the process tree.
    }
    spike = null;
  }
  console.log('[focus-sync] deactivated');
};

// Module shape required by host (matches `ExtensionModule` in @sigma-file-manager/api).
export default { activate, deactivate };