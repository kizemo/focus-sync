/**
 * Focus Sync v0.5.5 — Sigma FM extension entry point.
 *
 * v0.5.5 (2026-10-06) — 1 line return false fix:
 *   - v0.5.4 GetForegroundWindow check was warn + continue (silent race condition fail).
 *   - v0.5.5 abort: if dialog not foreground, return false → trigger H2 (UIA SetValue).
 *   - H2 works without foreground; UIA SetValue does not require foreground window.
 *   - 50ms flicker between SetValue target_path and SetValue original_filename.
 *
 * v0.5.4 (2026-10-05) — Home+Shift+End fix + I1 verify:
 *   - Ctrl+A 不 work in WinUI 3 wrapped Save As (WinUI 3 intercepts).
 *   - Fix: SendInput uses Ctrl+L + Home + Shift+End (basic Edit ops).
 *   - I1: post-SendInput focus verify (GetFocus + UIA) detects pollution.
 *
 * v0.5.2 (2026-10-05) — Ctrl+A fix for address bar concatenation:
 *   - Ctrl+A 也不 work in WinUI 3 wrapped Save As (WinUI 3 intercepts).
 *   - v0.5.4 改用 Home + Shift+End(基本 Edit 操作,WinUI 3 不太可能拦截)。
 *
 * v0.5.1 (2026-10-05) — Ctrl+L path works + Rule 47 (no auto-Save):
 *   - 4-layer fallback: COM SetFolder / H1 SendInput / H2 write+restore / F3 v0.5.0
 *
 * v0.5.0 (2026-10-04) — Sep 27 baseline rollback (dual-role bug):
 *   - uia_inject.rs 重写为 Sep 27 简单逻辑
 *
 * v0.4.0 撤回(用户反馈 "rollback to manual deployment success"):
 *   v0.4.0 加了 8+ 个 helper 和 address bar fallback,在 WinUI 3 wrapped
 *   dialog 上返回 Unsupported(什么都不做)。Sep 27 简单代码反而 work。
 *
 * v0.3.9 撤回:is_winui3_wrapped 检测是 regression(把 v0.3.8 working 的
 *   Edge WinUI 3 dialog 直接 skip)。
 *
 * 用户接受 trade-off (Sep 27 era):filename 字段可能显示路径 2-6 秒后 dialog
 * 自动关闭。这是 Sep 27 spike-report PASS 的核心行为。
 *
 * v0.3.8 (focus-21 + Mavis 10th-round A4-A9 review) — 沿用其 should_commit=true
 *   默认 + 1001/1148 helper,但 v0.5.0 简化了所有验证步骤。
 *
 * v0.3.7 (focus-21 + Mavis 8th-round L1 review) — REAL ROOT-CAUSE FIX:
 *   5+ rounds of focus-sync fixes failed because focus-20/21 wrote the
 *   target path to the WinUI filename edit and the user repeatedly
 *   rejected "filename STILL being modified" as visual pollution.
 *   v0.3.6 aligns with the user's stated preference: focus-sync EITHER
 *   fully navigates the dialog (Chromium 41477 branch) OR it does nothing.
 *   For WinUI Save As (no 41477, no SetFolder, has Edit element) the
 *   sidecar now returns `Unsupported` and bumps the counter; the extension
 *   shows a clear "enable edge://flags/#edge-legacy-file-picker" tip.
 *   The user keeps full control of the filename field.
 *
 * v0.3.5 (focus-21 + Mavis 7th-round review) additions over v0.3.4:
 *   - 30s boot grace period (N2 fix): after activate(), suppress error/warning
 *     notifications for 30s to prevent the "Sidecar not reachable" /
 *     "unsupported_dialog_count changed" cascade that was overwhelming
 *     first-time users. Edge tip itself remains ungated (critical info).
 *   - resetEdgeTip now immediately re-shows the tip (N5 fix) instead of
 *     asking the user to restart Sigma FM. Duration bumped to 12s.
 *   - loadBool helper (N7 fix): unified try/catch + default-value storage
 *     reads for enabled / autoConfirm / edgeLegacyFilePickerTipShown.
 *
 * v0.3.4 (focus-21) additions:
 *   - Toolbar menu item "🔁 Re-show Edge tip" (Bug B fix). Resets the
 *     `edgeLegacyFilePickerTipShown` flag so users who missed the
 *     one-time notification (or whose duration expired) can re-trigger
 *     it via Sigma FM's toolbar without re-installing.
 *   - Initial ping delayed 5s (Bug C fix). Scheduled Task mode means the
 *     sidecar may take a few seconds to come up; without the delay, users
 *     see "Sidecar not reachable" right next to the Edge tip notification,
 *     causing panic on first activation.
 *   - Edge tip duration 25s → 8s (P3 improvement §10.2). 25s was too long;
 *     the toolbar reset button (above) is the durable entry, not the long toast.
 *
 * v0.3.3 (focus-20) additions:
 *   - One-time Edge legacy file picker tip shown on first activate
 *     (`edgeLegacyFilePickerTipShown` storage flag). Educates user about
 *     `edge://flags/#edge-legacy-file-picker` which makes Edge use the
 *     classic IFileDialog and gives focus-sync full path-sync support.
 *     Per retrospective review §2.2 [P0-2]: this is the PRIMARY workaround
 *     for Edge Modern Save As (WinUI 3 in Win11 22H2+).
 *
 * v0.3.2 (focus-19) additions:
 *   - Polls /health for `unsupported_dialog_count` (a new focus-19 metric).
 *     When the count increases since the last poll AND >= 5 minutes have
 *     elapsed since the last notification, shows a "this dialog isn't
 *     supported" notification so the user understands why their Save As
 *     navigation didn't happen.
 *   - Deep-analysis §4.2 Option B (short-term WinUI Save As fix): sidecar
 *     returns Unsupported instead of polluting the filename field.
 *
 * v0.3.1 (focus-18) additions:
 *   - `autoConfirm` state (default false): when true, sidecar Invoke Save
 *     once per dialog session. Toolbar menu "Auto-Confirm" toggles this.
 *   - pushNow body adds `auto_confirm: autoConfirm` field; sidecar SetPathBody
 *     serde-deserializes with #[serde(default)] so legacy callers without
 *     the field still work (treated as auto_confirm=false).
 *
 * v0.3.0 architecture (Scheduled Task mode):
 *   - Sidecar is launched by Windows Task Scheduler at user logon, NOT by
 *     Sigma FM. Extension does NOT spawn the sidecar.
 *   - Extension only does HTTP health-check via sigma.http.request to
 *     127.0.0.1:37421/health, and POST /set_path on path change.
 *   - No need for sidecar binary path — HTTP IPC has no path dependency.
 *
 * Sandbox notes:
 *   - `fetch` is intercepted by Sigma FM extension sandbox (regex match).
 *     Use `sigma.http.request` (manifest `http` permission) instead.
 *   - `setInterval` may be sandbox-blocked in some scenarios. Use chain of
 *     setTimeout as a defensive fallback.
 *   - `sigma.fs.stat` / `sigma.registry.read` do NOT exist — we do NOT
 *     discover sidecar binary path; HTTP ping is path-independent.
 */

const SIDECAR_HTTP = 'http://127.0.0.1:37421';
const HEALTH_CHECK_INTERVAL_MS = 15_000;
const HEALTH_CHECK_TIMEOUT_MS = 5_000;
const UNSUPPORTED_NOTIFY_THROTTLE_MS = 5 * 60 * 1000; // 5 minutes

// v0.3.5 (Mavis N2): boot grace period — after activate(), suppress
// error/warning notifications for 30s. Reason: Scheduled Task mode means
// sidecar may take 5-15s to bind 37421, AND the first periodic health
// check fires at t=15s, AND checkHealth may surface an
// unsupported_dialog_count notification on the very first observation.
// Without this gate, first-activation UX was "Edge tip + Sidecar not
// reachable + unsupported count" = 3 notifications in 30s. With grace
// period: only the Edge tip shows, the others wait until stable.
//
// User actions still work during grace period (Sync Now, Auto-Confirm toggle,
// resetEdgeTip) — only the auto-fired error/warning banners are suppressed.
const BOOT_GRACE_MS = 30_000;
let bootStartedAt = 0; // set in activate()
function inGracePeriod() {
    if (bootStartedAt === 0) return false;
    return (Date.now() - bootStartedAt) < BOOT_GRACE_MS;
}

let enabled = true;
let autoConfirm = false; // focus-18: default OFF (safe)
let pushTimer = null;
let healthCheckTimer = null;
let notifiedSinceLastFailure = false;
// focus-19: track the last-seen unsupported_dialog_count from /health so we
// only notify on INCREASE (sidecar's counter is monotonic for the process
// lifetime). Initial value -1 means "never observed"; first poll establishes
// the baseline and never notifies.
let lastUnsupportedDialogCount = -1;
let lastUnsupportedNotifyTs = 0;

function normalizePath(p) {
    if (typeof p !== 'string') return p;
    let stripped = p.replace(/^sfm:\/?/i, '');
    return stripped.replace(/\//g, '\\');
}

function schedulePush(path) {
    if (pushTimer !== null) clearTimeout(pushTimer);
    pushTimer = setTimeout(() => {
        pushTimer = null;
        void pushNow(path);
    }, 100);
}

async function pushNow(path) {
    if (!enabled) return;
    const normalized = normalizePath(path);
    console.log('[focus-sync] PUSH raw=' + JSON.stringify(path) + ' normalized=' + JSON.stringify(normalized));

    try {
        // v0.3.0: NO retry with backoff. Previous v0.2.0 retries were a
        // workaround for Tauri callback race after extension runtime reload.
        // Now that the sidecar is independent of Sigma FM lifecycle, the
        // push either works (sidecar alive) or doesn't (sidecar down).
        // Retries would just spam the dying sidecar. One shot, log if failed.
        const r = await sigma.http.request({
            url: `${SIDECAR_HTTP}/set_path`,
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ path: normalized, auto_confirm: autoConfirm }),
        });
        console.log('[focus-sync] PUSH response status=' + r.status);
    } catch (e) {
        console.warn('[focus-sync] push failed:', String(e).slice(0, 200));
        // Notification handled by health check; don't double-notify here.
    }
}

/**
 * v0.3.0 health check. Uses sigma.http.request (sandbox-safe; NOT fetch).
 * Returns true if sidecar is reachable and healthy, false otherwise.
 */
async function pingSidecar() {
    try {
        const r = await sigma.http.request({
            url: `${SIDECAR_HTTP}/health`,
            method: 'GET',
            timeout: HEALTH_CHECK_TIMEOUT_MS,
        });
        if (r.status !== 200) return false;
        const body = JSON.parse(r.body || '{}');
        // v0.3.0: status field semantics
        //   "ok"       → fully operational
        //   "degraded" → process alive but UIA may be broken or session 0
        //   "down"     → unreachable
        return body.status !== 'down' && body.status !== undefined;
    } catch (e) {
        return false;
    }
}

/**
 * Health check + notification on first failure.
 *
 * v0.3.0 H1.1: notify on FIRST failure (not after N consecutive). User
 * should see the problem as soon as sync breaks.
 *
 * v0.3.0 V3.8: setInterval fallback → chain of setTimeout. Defensive against
 * sandbox possibly blocking setInterval.
 *
 * focus-19: ALSO checks the new `unsupported_dialog_count` field and shows
 * a notification when it increases since the last poll (throttled to once
 * per UNSUPPORTED_NOTIFY_THROTTLE_MS so a stream of WinUI Save As dialogs
 * does not spam the user).
 */
async function checkHealth() {
    const r = await pingSidecarWithBody();
    if (!r || !r.body) {
        // Ping failed entirely — handled below by treating as failure.
        return;
    }
    notifiedSinceLastFailure = false;

    // focus-19: parse unsupported_dialog_count and notify on increase.
    let parsed = null;
    try { parsed = JSON.parse(r.body); } catch (e) { parsed = null; }
    if (parsed && typeof parsed.unsupported_dialog_count === 'number') {
        const newCount = parsed.unsupported_dialog_count;
        if (lastUnsupportedDialogCount === -1) {
            // First observation: establish baseline, never notify.
            lastUnsupportedDialogCount = newCount;
        } else if (newCount > lastUnsupportedDialogCount) {
            const now = Date.now();
            // v0.3.5 (Mavis N2): suppress during boot grace period so
            // first-activation UX isn't a cascade of banners.
            if (!inGracePeriod() && now - lastUnsupportedNotifyTs >= UNSUPPORTED_NOTIFY_THROTTLE_MS) {
                try {
                    sigma.ui.showNotification({
                        title: 'Focus Sync',
                        description:
                            'Detected ' + (newCount - lastUnsupportedDialogCount) +
                            ' Save As dialog(s) the sidecar could not navigate ' +
                            '(e.g. Edge Win11 WinUI). Path sync skipped to avoid ' +
                            'polluting the filename field. See focus-19 plan.',
                        type: 'warning',
                        duration: 8000,
                    });
                } catch (e) {}
                lastUnsupportedNotifyTs = now;
            }
            lastUnsupportedDialogCount = newCount;
        }
    }
}

/**
 * Ping sidecar and return the full response object (not just ok bool).
 * focus-19: callers need the body to read `unsupported_dialog_count`.
 * Returns null on transport failure (used by checkHealth above to decide
 * whether to notify the sidecar-unreachable banner).
 */
async function pingSidecarWithBody() {
    try {
        const r = await sigma.http.request({
            url: `${SIDECAR_HTTP}/health`,
            method: 'GET',
            timeout: HEALTH_CHECK_TIMEOUT_MS,
        });
        if (r.status !== 200) return null;
        return r;
    } catch (e) {
        // Transport failure: notify on first occurrence (banner pattern).
        // v0.3.5 (Mavis N2): suppress during boot grace period so the
        // banner doesn't fire next to the Edge tip during first activation.
        if (!notifiedSinceLastFailure && !inGracePeriod()) {
            try {
                sigma.ui.showNotification({
                    title: 'Focus Sync',
                    description:
                        'Sidecar not reachable on 127.0.0.1:37421.\n' +
                        'Open Task Scheduler → KizemoFocusSync → Run, or restart your PC.',
                    type: 'error',
                    duration: 15000,
                });
            } catch (e2) {}
            notifiedSinceLastFailure = true;
        }
        return null;
    }
}

function startHealthCheck() {
    if (healthCheckTimer !== null) return; // already running
    if (typeof setInterval === 'function') {
        healthCheckTimer = setInterval(() => { void checkHealth(); }, HEALTH_CHECK_INTERVAL_MS);
    } else {
        // Fallback: chain of setTimeout
        const tick = () => {
            void checkHealth().finally(() => {
                healthCheckTimer = setTimeout(tick, HEALTH_CHECK_INTERVAL_MS);
            });
        };
        healthCheckTimer = setTimeout(tick, HEALTH_CHECK_INTERVAL_MS);
    }
}

function stopHealthCheck() {
    if (healthCheckTimer === null) return;
    if (typeof clearInterval === 'function') clearInterval(healthCheckTimer);
    else clearTimeout(healthCheckTimer);
    healthCheckTimer = null;
}

async function syncNow() {
    try {
        const path = await sigma.context.getCurrentPath();
        if (!path) {
            try {
                sigma.ui.showNotification({ title: 'Focus Sync', description: 'No current folder.', type: 'warning' });
            } catch (e) {}
            return;
        }
        await pushNow(path);
        try {
            sigma.ui.showNotification({ title: 'Focus Sync', description: 'Synced: ' + path, type: 'success' });
        } catch (e) {}
    } catch (e) {}
}

/**
 * v0.3.5 (Mavis N7): unified storage getter for boolean flags. Returns the
 * stored value if present (must be exactly `true`/`false`, not e.g. string),
 * otherwise the default. Catches any storage error (sandbox / not-allowed)
 * and falls back to the default — same defensive behavior as the inline
 * try/catches it replaces.
 *
 * Use for: enabled / autoConfirm / edgeLegacyFilePickerTipShown. Do NOT use
 * for path-like strings (normalizePath-style) where `?? default` semantics
 * would mask a missing entry as the default path.
 */
async function loadBool(key, defaultValue) {
    try {
        const v = await sigma.storage.get(key);
        // Only treat literal `true` / `false` as overrides; if sigma.storage
        // returns something else (undefined / null / string), use default.
        return (v === true || v === false) ? v : defaultValue;
    } catch (e) {
        return defaultValue;
    }
}

async function toggle() {
    try {
        enabled = !enabled;
        await sigma.storage.set('enabled', enabled);
        try {
            sigma.ui.showNotification({ title: 'Focus Sync', description: enabled ? 'Enabled' : 'Disabled', type: 'info' });
        } catch (e) {}
    } catch (e) {}
}

/**
 * focus-18: Toggle Auto-Confirm. When ON, the next PUSH carries
 * auto_confirm=true and the sidecar Invoke Save on the first push to a
 * given dialog. Per-dialog dedup is on the sidecar (state.committed_hwnds).
 */
async function toggleAutoConfirm() {
    try {
        autoConfirm = !autoConfirm;
        await sigma.storage.set('autoConfirm', autoConfirm);
        try {
            sigma.ui.showNotification({
                title: 'Focus Sync',
                description: 'Auto-Confirm: ' + (autoConfirm
                    ? 'ON (Save button auto-clicked after sync — v0.3.8 default)'
                    : 'OFF (filename written + Save button auto-clicked, v0.3.8 always commits)'),
                type: autoConfirm ? 'warning' : 'info',
                duration: 5000,
            });
        } catch (e) {}
    } catch (e) {}
}

/**
 * focus-21 Bug B + v0.3.5 (Mavis N5) fix: durable re-trigger for the Edge tip.
 *
 * The notification is one-shot (gated by `edgeLegacyFilePickerTipShown`) so
 * users who missed the 8s toast have no way to see it again. This toolbar
 * entry IMMEDIATELY re-shows the full tip (12s duration — slightly longer
 * than the default 8s because the user explicitly asked for it) and re-sets
 * the storage flag to true to prevent double-show on the next activate().
 *
 * v0.3.4 had this say "Edge tip will show on next activation" — but
 * activate() only runs on Sigma FM restart, so users had to bounce Sigma FM
 * to see the tip. Confusing UX. v0.3.5 shows it inline.
 */
async function resetEdgeTip() {
    try {
        // Re-show the full tip immediately (12s; longer than the 8s default
        // because the user actively asked for it).
        try {
            sigma.ui.showNotification({
                title: 'Focus Sync — Edge Save As tip',
                description:
                    'Edge "Modern Save As" (Win11 22H2+) has limited path sync.\n' +
                    '\n' +
                    'To enable full sync:\n' +
                    '1. Open Edge, type edge://flags/#edge-legacy-file-picker in address bar\n' +
                    '2. Find "Legacy File Picker" → set to "Enabled"\n' +
                    '3. Restart Edge\n' +
                    '\n' +
                    'After restart, Save As uses the classic IFileDialog and focus-sync fully supports it.\n' +
                    '\n' +
                    'To revert: same URL → set to "Default" → restart Edge.',
                type: 'info',
                duration: 12000,
            });
        } catch (e) {}
        // Re-set the flag so the next activate() does NOT show the tip
        // again (avoid double-notification on Sigma FM restart).
        try {
            await sigma.storage.set('edgeLegacyFilePickerTipShown', true);
        } catch (e) {}
    } catch (e) {
        // Storage may be unavailable in some sandbox contexts; non-fatal.
    }
}

const activate = async () => {
    // v0.3.5 (Mavis N2): stamp boot start so the 30s grace period gate works.
    bootStartedAt = Date.now();
    console.log('[focus-sync] v0.5.5 activate START (H1 SendInput abort if !foreground + H2 UIA SetValue fallback + L1 .no_proxy() + find_ifiledialog_edit 1001/1148 + should_commit=true (unused per Rule 47) + verify + path tolerance)');
    try { await sigma.i18n.mergeFromPath('locales'); } catch (e) {}
    // v0.3.5 (N7): unified loadBool for storage reads. Was 2 inline try/catch
    // blocks; Bug B reset adds a 3rd (`edgeLegacyFilePickerTipShown`).
    enabled = await loadBool('enabled', true);
    autoConfirm = await loadBool('autoConfirm', false);
    const tipShown = await loadBool('edgeLegacyFilePickerTipShown', false);

    // v0.3.0: NO sidecar spawn. Sidecar runs as Windows Scheduled Task
    // 'KizemoFocusSync' (AtLogOn, --service flag). We only connect via HTTP.

    // focus-20 (2026-10-02): show Edge legacy file picker flag tip ONCE on
    // first activation. Per retrospective review §2.2 [P0-2]: this is the
    // PRIMARY workaround for Edge Modern Save As (WinUI 3) which the
    // spike sidecar's UIA path cannot fully navigate. Enabling the flag
    // makes Edge use the classic IFileDialog (`#32770` with full COM
    // ServiceProvider) which focus-sync supports end-to-end.
    //
    // Shown once via storage flag (`edgeLegacyFilePickerTipShown`) so the
    // user sees it on first install but is not re-bombarded on every
    // Sigma FM restart.
    //
    // focus-21 (2026-10-02) updates:
    //   - duration: 25s → 8s (per §10.2). 25s was too long; the toolbar
    //     "🔁 Re-show Edge tip" entry (below) is the durable way to revisit.
    //   - The toolbar reset button (resetEdgeTip) sets the storage flag
    //     back to false so the tip shows again on next activation.
    try {
        // tipShown loaded via loadBool at the top of activate() (N7).
        if (tipShown !== true) {
            sigma.ui.showNotification({
                title: 'Focus Sync — Edge Save As tip',
                description:
                    'Edge "Modern Save As" (Win11 22H2+) is NOT auto-navigated by focus-sync v0.3.6 — ' +
                    'the filename field is yours, we will not write into it.\n' +
                    '\n' +
                    'To enable full sync (focus-sync auto-fills path and you click Save):\n' +
                    '1. Open Edge, type edge://flags/#edge-legacy-file-picker in address bar\n' +
                    '2. Find "Legacy File Picker" → set to "Enabled"\n' +
                    '3. Restart Edge\n' +
                    '\n' +
                    'After restart, Save As uses the classic IFileDialog and focus-sync ' +
                    'fully supports it (path auto-filled, no manual edits).\n' +
                    '\n' +
                    'To revert: same URL → set to "Default" → restart Edge.\n' +
                    '\n' +
                    'Missed this? Use the Focus Sync toolbar menu → "🔁 Re-show Edge tip".',
                type: 'info',
                duration: 12000,
            });
            await sigma.storage.set('edgeLegacyFilePickerTipShown', true);
        }
    } catch (e) {
        // sigma.ui / storage may be unavailable in some sandbox contexts;
        // non-fatal — health check will still warn on sidecar failure.
    }


    // Initial ping (non-blocking) DELAYED 5s + schedule periodic health check.
    //
    // focus-21 Bug C fix: in Scheduled Task mode the sidecar takes a few
    // seconds to come up. Without the delay, first-activate UX was two
    // notifications fired back-to-back:
    //   1. Edge tip (info, 8s now)
    //   2. Sidecar not reachable (error, 15s)
    // The red "not reachable" banner right next to the new-tip notification
    // made users panic on first install. Delaying the initial ping by 5s
    // gives the Scheduled Task enough room to bind 37421 before we check.
    //
    // Periodic health check (startHealthCheck) keeps its normal cadence;
    // this delay ONLY applies to the very first ping.
    setTimeout(() => {
        void pingSidecar().then((ok) => {
            if (!ok && !notifiedSinceLastFailure && !inGracePeriod()) {
                try {
                    sigma.ui.showNotification({
                        title: 'Focus Sync',
                        description:
                            'Sidecar not reachable. Open Task Scheduler → KizemoFocusSync → Run.',
                        type: 'error',
                        duration: 15000,
                    });
                    notifiedSinceLastFailure = true;
                } catch (e) {}
            }
        });
    }, 5000);
    startHealthCheck();

    try {
        sigma.commands.registerCommand({ id: 'focus-sync.syncNow', title: 'Sync Now' }, syncNow);
        sigma.commands.registerCommand({ id: 'focus-sync.toggle', title: 'Enable / Disable' }, toggle);
        sigma.commands.registerCommand({ id: 'focus-sync.toggleAutoConfirm', title: 'Toggle Auto-Confirm' }, toggleAutoConfirm);
        // focus-21 Bug B: durable entry point to re-trigger the Edge tip
        // notification. Resets the storage flag so the next activate()
        // shows the tip again.
        sigma.commands.registerCommand({ id: 'focus-sync.resetEdgeTip', title: 'Re-show Edge tip' }, resetEdgeTip);
    } catch (e) {}

    try {
        sigma.toolbar.registerDropdown({
            id: 'focus-sync.toolbar',
            title: 'Focus Sync',
            icon: 'mdi-target',
            items: [
                { id: 'syncNow', title: 'Sync Now', commandId: 'focus-sync.syncNow' },
                { id: 'separator', title: '', separator: true },
                { id: 'toggle', title: 'Enable / Disable', commandId: 'focus-sync.toggle' },
                { id: 'separator2', title: '', separator: true },
                // focus-18: toolbar title cannot be dynamically updated (Sigma FM
                // freezes labels at registerDropdown time). The notification
                // after toggling carries the current state — user remembers.
                { id: 'toggleAutoConfirm', title: 'Auto-Confirm (currently: ' + (autoConfirm ? 'ON' : 'OFF') + ')', commandId: 'focus-sync.toggleAutoConfirm' },
                { id: 'separator3', title: '', separator: true },
                // focus-21 Bug B: durable entry to re-show Edge tip. Users who
                // missed the 8s toast (or have reinstalled) get a one-shot
                // "Edge tip will show on next activation" confirmation.
                { id: 'resetEdgeTip', title: '🔁 Re-show Edge tip', commandId: 'focus-sync.resetEdgeTip' }
            ]
        }, { syncNow, toggle, toggleAutoConfirm, resetEdgeTip });
    } catch (e) {}

    try {
        sigma.context.onPathChange((path) => {
            console.log('[focus-sync] onPathChange raw=' + JSON.stringify(path));
            if (path) schedulePush(path);
        });
    } catch (e) {}

    try {
        const initialPath = await sigma.context.getCurrentPath();
        if (initialPath) schedulePush(normalizePath(initialPath));
    } catch (e) {}
};

const deactivate = async () => {
    if (pushTimer !== null) { clearTimeout(pushTimer); pushTimer = null; }
    stopHealthCheck();
    // v0.3.0: do NOT call /quit — the sidecar is owned by Task Scheduler,
    // not by us. Shutting it down here would defeat the whole point of
    // Scheduled Task mode.
};

var index = { activate, deactivate };
export { activate, deactivate, index as default };
