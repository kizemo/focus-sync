# Focus Sync — Extension Changelog

**适用范围**:`kizemo.focus-sync` Sigma FM extension 及其 sidecar(`focus-sync-sidecar`)。

> **为什么这份文档存在**
> 2026-10-07 之前,v0.3.0 → v0.5.5 的全部变更历史以**注释**形式写在
> `release/extension/dist/index.js` 的文件头。2026-10-07 的 Phase 4 修复中,
> 头部注释里的一句 `foreground window.` 命中了 Sigma FM 沙箱正则 `/\bwindow\s*\./g`,
> 导致 extension 被 `loader.ts` 静默拒绝、永不加载(sidecar 108 分钟 0 请求)。
>
> **结论:变更历史不应该放在会被沙箱正则扫描的源文件里。**
> 它属于文档。现在它在这里;`index.js` 的头部只保留 4 行指针注释。
>
> 详见 `docs/superpowers/plans/2026-10-07-focus-21-phase4-sandbox-fix-verification.md`。

---

## v0.5.8 (2026-10-08)

**BUG-2 根因更正与修复 + 关闭「已知限制」中的 4 项**

本次的起点是一个已被记录的错误结论:`known-issues-2026-10-08.md` 认定 BUG-2 是
「H2 回退时还原不完整」。**全量日志统计证明写脏文件名框的是 F3 基线路径,不是 H2。**
详细证据见该文档 BUG-2 小节。一句话版本:F3 把完整路径写进文件名框之后**根本没有还原**,
而 H2 在故障窗口内一次都没执行过。

### sidecar

| 改动 | 内容 |
|---|---|
| **BUG-2** | F3 基线改为:写前快照 → 写 `target_path` → 50ms → **还原原文件名 → 读回证明**。两次机会,仍失败则 `error!` 报出当前脏值 |
| **H2 假成功** | 新增 `restore_filename_verified()`,H2 的还原从「`SetValue` 返回 Ok」升级为「读回值完全相等」。**还原失败时返回 false**,不再谎报成功 |
| **死 HWND 累积** | `inject_folder_path` 入口先查 `IsWindow`,是死句柄直接返回新的 `DialogOutcome::DialogGone`;`writer` 随即把它从注册表摘除。**绝不累加 `unsupported_dialog_count`** |
| **版本号可辨识** | `SPIKE_VERSION` 从 `0.5.5`(自 v0.5.5 起就没再动过,横跨 v0.5.6/v0.5.7)改为 `0.5.8`,`/health` 现在能报出真实运行版本 |
| **测试副作用** | `kill_holding_sidecar()` 在 `cfg!(test)` 下短路。**修复前每跑一次 `cargo test` 都会 `taskkill /F` 掉你正在运行的 sidecar**(2026-10-08 实测,杀掉了 PID 598276) |
| **测试断言** | `resolve_port_strict_on_conflict` 原标 `#[should_panic]`,但 `resolve_port` 是返回 `Err` 不是 panic,该断言永远不可能成立 —— 改为断言 `is_err()` |

### 插件(`dist/index.js`)

| 改动 | 内容 |
|---|---|
| **P1 推送无 trace** | 见下,这是本次最反直觉的一条 |
| **P3 `getCurrentPath()` 返回 null** | 新增 `resolveCurrentPath()`:先问 `getCurrentPath()`,拿不到就回落 `lastKnownPath`。工具栏「Sync Now」原本每次都只能弹「No current folder.」 |

---

### P1 的真实机理:证据被观测行为本身冲掉了

复盘记录的现象是「sidecar 收到了 `/set_path`,但 trace 里没有 N05/N06/N07/N08,
且 seq 连续无缺口」。实测 trace 环(300 条)后发现:

```
N13.notify.decide      150
N03.health.resp        150
推送路径节点            0        ← 一条都没有
```

**环被健康轮询 100% 占满了。** 健康检查每 15s 产生一对节点,其中 N03 的
`bodyFp` 每次展开成约 250 个字符的码元数字列表。按这个速率,37 分钟就能把
300 条的环冲掉三轮 —— 而 `seq` 之所以「连续无缺口」,是因为**每一条都是健康节点**,
推送节点在落盘前就被挤出去了。

⇒ **不是 trace 丢了,是被覆盖了。** 观测行为本身销毁了待观测的证据。

修复分两半:
- **止血**:N13 只在计数变化/通知被抑制/建立基线时记录(原本每次轮询都记,且 payload
  恒为 `increased:false`);N03 健康路径不再记录 `bodyFp`(`bodyType` 已足够证明
  G-7 的结论,`bodyFp` 只在解析失败时才真正有诊断价值,那仍由 N12 保留)
- **可判定**:新增 `N18.schedule`(调度入口)与 `N19.push.enter`(`pushNow` 第一行,
  在 `enabled` 判断之前)。现在「回调没触发 / 定时器没触发 / pushNow 跑了但 trace 丢了」
  三种情况在日志里长得不一样

---

## v0.5.8 续 —— 同日第二轮(四个新根因)

上一节记的是「计划中的修复」。下面四条是**部署后被用户实测打出来**的,每条都在用户界面上出现过才修。

### 1. 重复写入(地址栏跳两次)

```
08:00:47.939  [monitor] dialog detected: hwnd=15467576
08:00:47.959  [monitor] regained foreground, last_written=''  -> resync
08:00:48.387  write_path commit=true
08:00:49.286  write_path commit=false          <-- 同一弹窗,同一个目标,900ms 后再写一遍
```

成因:`last_resync_at` 只在 `should_resync && !should_push` 时才设置,而**弹窗首次检测走的是 `should_push` 分支**,于是没有武装冷却。8ms 后的下一个 tick 发现 `last_written` 还是空(第一次写入要 ~400ms,main 尚未登记),就又满足一次 resync。

**修法**:`last_resync_at` 改为**每次发送事件都设置**。1200ms 冷却本来就存在,只是没在这条路径上生效。

> 这正是 v0.5.7 DEDUP-1 的盲区 —— DEDUP-1 加在 `/set_path`,没加在 monitor 这条路径上。

### 2. 窗口激活过渡导致「操作已派发但没导航」

```
16:10:34.531  fg=#32770 另存为        <- 弹窗前台
16:10:35.412  fg=ForegroundStaging    <- Windows 激活过渡的临时窗口
16:10:35.553  fg=#32770 另存为        <- 弹窗恢复
08:10:35.412  H1: no focus element resolvable
08:10:35.511  H1: ABORTING before dispatching any keys (BUG-1 gate)
08:10:35.621  H2: ... restored+verified  ->  Injection succeeded
```

Windows 把窗口激活路由经过一个叫 `ForegroundStaging` 的临时窗口。sidecar 恰好在它存在的 ~150ms 内采样:H1 拿不到焦点元素 → 回退键盘 → BUG-1 闸门**正确拦下**(那一刻前台确实不是弹窗)→ 落到 H2。而 **H2 写文件名框再还原从来不是导航**(Claude Rule 58),于是用户什么都没看到,sidecar 却记了 `Injection succeeded`。

**BUG-1 闸门是工作正常的**,问题是它只报失败、不重试,而闸门后面唯一的选择是一个无效的 H2。

**修法**:不放宽闸门,**在闸门上等**。新增 `await_dialog_foreground()`,闸门判定失败时轮询等待弹窗重新成为前台(上限 500ms)。**每个 `SendInput` 批次前仍逐次校验前台**,全程不抢焦点、不发按键,弹窗没回来照样拒绝。5 处闸门全部接入。

> `ForegroundStaging` 这条**只有独立观察进程才看得见**。sidecar 自己的日志只记「dialog not foreground」这个结果,不记「当时前台到底是什么」。拿 sidecar 的日志查 sidecar 是循环论证。
> 后续实测:该过渡窗口在**普通应用切换**时也会出现(豆包 ↔ 微信 ↔ 任务切换器),不是弹窗特有。

### 3. 存活信号用了共享诊断端点(陈旧路径注入)

sidecar 由计划任务常驻,生命周期长于 Sigma FM,`current_path` 是「最后一次推送」。Sigma FM 关闭、或刚重启而扩展尚未激活时,那个路径就是陈旧的,而 sidecar 仍会注入 —— 用户看到「打开弹窗就被填了旧地址」。

实测扩展激活延迟:**15:34:11 启动 → 15:37:09 激活(2m58s)**;16:31:41 启动 → 100s 后仍未激活。

**修法**:用「扩展是否存活」作为门槛。插件每 15s 轮询一次,sidecar 以此判断该路径是否还值得注入;超窗则**拒绝写入**,返回独立错误码 `extension_not_alive`(不复用 `unsupported_dialog_type`,不累加计数器)。

> **这比「路径超过 N 分钟即过期」好得多**:后者会误伤「导航一次然后专心下载一小时」——只要 Sigma FM 开着,轮询就一直来,同步就一直有效。只有真正关掉、或还没起来的扩展才停止注入。

### 4. 存活信号端点选错 + 观测值在盖章后测量

第 3 条的**第一版实现是错的**,两处:

**(a) 用 `/health` 当存活信号。** 它是**共享诊断端点** —— `read-sidecar-log.ps1`、临时 `Invoke-WebRequest`、每一次调试会话都会调它。证据:静默 75 秒后 `last_health_poll_age_ms=20038`,精确等于观测者自己两次读取的间隔,期间无第三方轮询。**等于拿被污染的信号当生命线。**
**修法**:新增插件专属端点 `GET /ext_alive`,只有插件会调;`/health` 不再盖章。

**(b) 在盖章之后才测量。**
```rust
state.mark_health_poll();                        // 盖章
let alive_after = state.extension_alive(window); // 紧接着量 → 恒为 true
```
于是 `/health` 报出来的 `extension_alive` **永远是 true**,闸门被自己的观测值完全掩盖(实测 `age=99360ms` 却报 `alive=true`,自相矛盾)。
**修法**:`/health` 只读不写,报真实存储值。

> **教训:一个你看不了的闸门,和一个没接线的闸门在外部完全无法区分。** 本项目历史上每一个静默失效(BUG-2 的注册表、死 HWND 计数器、P1 的 trace 环被挤满)都是这个形状。现在闸门开合都会打日志,并且输入与判定都从 `/health` 可读。

---

## v0.5.5 (2026-10-06)

**1 行 return false 修复**

- v0.5.4 的 `GetForegroundWindow` 检查是「warn + 继续」——静默的竞态失败。
- v0.5.5 改为:对话框不在前台时 **直接 `return false`**,从而触发 H2(UIA SetValue)。
- H2 不依赖前台状态;UIA SetValue 不要求前台窗口。
- `SetValue target_path` 与 `SetValue original_filename` 之间有 50ms 闪烁。

---

## v0.5.4 (2026-10-05)

**Home+Shift+End 修复 + I1 校验**

- `Ctrl+A` 在 WinUI 3 封装的 Save As 对话框里无效(WinUI 3 会拦截)。
- 修复:`SendInput` 改用 `Ctrl+L` → `Home` → `Shift+End`(基础 Edit 操作)。
- I1:`SendInput` 之后的焦点校验(`GetFocus` + UIA)用于检测焦点污染。

## v0.5.2 (2026-10-05)

**地址栏拼接问题的 Ctrl+A 修复尝试**

- `Ctrl+A` 同样在 WinUI 3 封装的 Save As 里无效。
- v0.5.4 改用 `Home` + `Shift+End`(基础 Edit 操作,WinUI 3 不太可能拦截)。

## v0.5.1 (2026-10-05)

- `Ctrl+L` 路径写入可用。
- **Rule 47(不自动 Save)**:4 层 fallback —
  COM `SetFolder` → H1 `SendInput` → H2 写入+还原 → F3(回退到 v0.5.0 逻辑)。

## v0.5.0 (2026-10-04)

- 回滚到 2026-09-27 基线(dual-role bug)。
- `uia_inject.rs` 重写为 9-27 的简单逻辑。

---

## v0.4.0 —— 已撤回

用户反馈「回滚到手动部署成功的版本」。

- v0.4.0 加了 8+ 个 helper 和地址栏 fallback,在 WinUI 3 封装的对话框上
  返回 `Unsupported`(什么都不做)。
- 结果:9-27 的简单代码反而能用。

## v0.3.9 —— 已撤回

- `is_winui3_wrapped` 检测被判定为 regression —— 它把 v0.3.8 本来 work 的
  Edge WinUI 3 对话框直接 skip 掉了。

## 用户接受的 trade-off(9-27 时期确立)

`filename` 字段可能显示路径后 2–6 秒对话框自动关闭。
这是 9-27 spike-report PASS 的核心行为。

---

## v0.3.8(focus-21 + Mavis 第 10 轮 A4–A9 review)

- 沿用 `should_commit=true` 默认值,以及 1001/1148 helper。
- v0.5.0 简化了以上所有验证步骤。

## v0.3.7(focus-21 + Mavis 第 8 轮 L1 review)—— **真正的根因修复**

- 5 轮以上 focus-sync 修复失败,原因是 focus-20/21 把目标路径写进了
  WinUI 的 filename 输入框,而用户反复把「filename 仍然被修改」当作视觉污染拒绝。
- v0.3.6 对齐用户明确表达的偏好:focus-sync **要么**完整导航对话框
  (Chromium 41477 分支),**要么什么都不做**。
- 对 WinUI Save As(无 41477、无 SetFolder、有 Edit 元素):
  sidecar 返回 `Unsupported` 并累加计数器;extension 显示明确的
  `edge://flags/#edge-legacy-file-picker` 提示。
- **用户始终完全掌控 filename 字段。**

## v0.3.5(focus-21 + Mavis 第 7 轮 review)

- **N2 — 30s 启动宽限期**:`activate()` 之后 30 秒内抑制 error/warning 通知,
  避免首次使用时「Sidecar not reachable」+「unsupported_dialog_count 变化」
  的通知风暴。Edge 提示本身不受门控(属于关键信息)。
- **N5**:`resetEdgeTip` 立即重新显示提示,不再要求用户重启 Sigma FM;时长 12s。
- **N7 `loadBool` helper**:把 `enabled` / `autoConfirm` /
  `edgeLegacyFilePickerTipShown` 的读取统一为「try/catch + 默认值」。
  ⚠️ 该函数**有** catch(返回默认值),失败不会中断 `activate()`。

## v0.3.4(focus-21)

- **Bug B**:工具栏菜单项「🔁 Re-show Edge tip」,重置
  `edgeLegacyFilePickerTipShown`,让错过一次性通知的用户无需重装即可重看。
- **Bug C**:首次 ping 延迟 5s。Scheduled Task 模式下 sidecar 需要几秒才起来;
  不延迟的话首次激活会同时弹出「Sidecar not reachable」和 Edge 提示,造成恐慌。
- Edge 提示时长 25s → 8s。25s 太长,工具栏重置按钮才是持久入口。

## v0.3.3(focus-20)

- 首次 `activate()` 时显示一次性 Edge legacy file picker 提示
  (由 `edgeLegacyFilePickerTipShown` 存储标记控制)。
- 该 flag 是 **Edge 现代版 Save As(WinUI 3)** 的**首选**变通方案。

## v0.3.2(focus-19)

- 轮询 `/health` 的 `unsupported_dialog_count`。
  当计数较上次观测增加、且距上次通知已过 ≥ 5 分钟时,提示用户
  「这个对话框不受支持」,解释为什么 Save As 没有被导航。
- 深度分析 §4.2 Option B:sidecar 返回 `Unsupported`,而不是污染 filename 字段。

## v0.3.1(focus-18)

- `autoConfirm` 状态(默认 false)。为 true 时,sidecar 在每个对话框会话中
  最多自动点一次 Save。工具栏菜单「Auto-Confirm」控制开关。
- `pushNow` 请求体新增 `auto_confirm` 字段;sidecar 的 `SetPathBody` 用
  `#[serde(default)]` 反序列化,旧调用方不带该字段也能工作。

## v0.3.0 —— 架构变更(Scheduled Task 模式)

- sidecar 由 **Windows 任务计划器**在用户登录时启动,**不再**由 Sigma FM 启动。
- extension 只通过 `sigma.http.request` 对 `127.0.0.1:37421/health` 做健康检查,
  以及在路径变化时 `POST /set_path`。
- manifest 移除 `shell` 权限和 `binaries[]` 段 —— 不再需要 sidecar 二进制路径,
  HTTP IPC 没有路径依赖。

---

## 沙箱约束(**永远不要放回 index.js**)

以下每一条都会让 Sigma FM 的 `validateExtensionCode()` 拒绝整个 extension,
且失败是**静默的**:没有 UI 报错、没有通知、sidecar 收不到任何请求。

- `fetch` 被沙箱正则拦截 → 用 `sigma.http.request`(需 manifest `http` 权限)。
- `setInterval` 在某些场景可能被拦 → 防御性地用 `setTimeout` 链。
- `sigma.fs.stat` / `sigma.registry.read` **不存在**。
- 沙箱会扫描**注释**,不只是代码。任何 `window.` 之类的写法哪怕在注释里也会中招。

### 硬性要求

```powershell
node scripts\scan-sandbox-dynamic.cjs release\extension\dist\index.js
```

必须输出 `VALID (0 violations / N patterns)`。
**所有 hook(构建 / 部署 / pre-commit)都会自动跑这条。**

规则由扫描器**从 `sandbox.ts` 实时提取**,不是硬编码快照 ——
`sandbox.ts` 在 git 历史中变更过(`ba3e5a3f` 新增了 `.constructor()`),
硬编码会静默失效。

---

## 已知未解决 / 待观察

> 详细证据链与修复方案见
> `docs/superpowers/plans/2026-10-07-focus-21-complete-retrospective-and-fix-plan.md`
> 与 `docs/superpowers/plans/2026-10-07-focus-21-response-to-claude-v0.5.6-plan.md`

| 项 | 状态 | 证据 / 激活条件 |
|---|---|---|
| **H1 只在对话框打开时有效** | **开放,待修复** | 全量日志统计:`H1-OK 48 / H1-FAIL 8`,**无一例外** —— 对话框刚打开(`opened_write_back`)时前台是它,H1 成功并真正导航;用户在 Sigma FM 里切目录(`/set_path`)时前台是 Sigma FM,H1 必然失败并退到 H2。H2 往 filename 控件写路径再还原,**按 Claude Rule 58 那不是导航** => 用户抱怨的是「打开弹窗时对、之后切目录就不动」。⚠️ **2026-10-08 更新**:DIRECT-1(直接 UIA SetValue 替换地址栏)改变了这个格局 —— `H1 succeeded` 全历史 86 次,其中 DIRECT 路径 5 次、键盘回退 81 次,**两者不等价**,DIRECT-1 成功时不需要前台地址栏焦点以外的额外条件。修订结论需要一次新的定向实验,不要沿用旧的 48/8 数字 |
| **COM SetFolder 从未成功** | 结构性 | 全部日志 `COM SetFolder succeeded = 0`(累计 unavailable 107+ 次,零代码验证法:开 `edge://flags/#edge-legacy-file-picker` 后重测,若立刻出现 succeeded,则 H1/H2/F3 回退链可退休) |
| **缺陷 B:告警通道整体失效** | ✅ 已修复 | `JSON.parse(/health body)` 失败 => `checkHealth()` 的 `unsupported_dialog_count` 整块被跳过。**G-7 已修**(`decodeHttpBody` 用 `ArrayBuffer.isView`,`instanceof` 跨 realm 不成立)。实测 2026-10-08:`N13.notify.decide` 正常记录,告警通道活着 |
| **缺陷 A:对死 HWND 写入** | ✅ 已修复(v0.5.8,**两轮才真正生效**) | 第一版把 `IsWindow` 检查放在 `inject_folder_path` 入口,返回独立错误码 `dialog_gone` 并就地摘除注册表 —— 但 `http_server` 的两个 `continue`(DEDUP 跳过、R-2 非前台延后)都发生在它之前,死句柄永远到不了检查处,实测 `active_dialogs` 仍涨到 3 而实际存活弹窗为 0。**第二版把清理移到 `handle_set_path` 循环体的最前面**,早于两个 `continue`,才真正生效(`http_server.rs:298` prune,先于 L316 / L356) |
| **`/health` 响应缺 `charset`** | ✅ 已修复(v0.5.8) | `Content-Type` 现为 `application/json; charset=utf-8`。sidecar 一直发的是 UTF-8,只是没说;客户端(如 PowerShell 5.1 的 `Invoke-WebRequest .Content`)会退回系统 ANSI 代码页 936,把 `E:\办公文件` 显示成 `E:\åŠå…æä»¶`。**2026-10-08 一天内因此浪费三次排查**,其中一次差点被当成编码 bug 去查一个不存在的乱码问题(`Invoke-RestMethod` 恰好解码正确、`Invoke-WebRequest` 不正确,这种分裂最误导) |
| **H1 键盘回退路径把路径重复输入两遍** | **开放,休眠中** | `uia_inject.rs` 的 `try_send_path_via_sendinput` 里有**两段完全相同**的「End + 128×Backspace + 逐字输入」代码块(L647-655 与 L678-686,前者是原有实现,后者是 G-5 补丁追加的,其注释描述的行为前者已实现)。净地址栏内容不变,但单次注入的事件数翻倍,并把「清空再输入」的可见时间窗拉长一倍。⚠️ **2026-10-08 17:06 复核:缺陷仍在源码中,但已被绕过 —— 激活等待修复之后(08:47Z 起)的 5 次 H1 尝试全部走 DIRECT-1,键盘回退 0 次;修复前今日 40 次尝试中键盘回退占 17 次。所以不是「已解决」,是「暂时走不到」:一旦 DIRECT-1 失败(对话框无可聚焦地址栏等),重复输入立即回来。** 用户 2026-10-08 决定本轮不改动(避免动已确认达标的 H1) |
| `__binaries["focus-sync-sidecar"].path = null` | 休眠 —— 部署源码中 `sigma.binary` 零匹配 | 任何转向「extension 启动 sidecar」的架构变更 |
| `build_http_client()` 无 `.no_proxy()` | 休眠 —— 当前 `ProxyEnable=0` | 用户开启系统代理(已配置 `127.0.0.1:7897`) |
| P2-1 HTTP -> stdio(省 ~340 行) | 暂缓 | 功能恢复验证通过后再议 |
| P2-2 用 Sigma 原生 binary(省 40 KB) | 暂缓 | 同上 |

### 更正记录

**更正 1 —— 「中文路径 mojibake」是误报。**
PowerShell 5.1 以 ANSI 代码页(936)读取 UTF-8 日志造成的**显示假象**,
编码链路实际完全正常(trace 全部节点判定为 `genuine`)。

**更正 2 —— 「缺陷 C:H1 被永久禁用」是误判。**
全量日志统计证伪:`H1 succeeded = 48` vs `failed = 8`,H1 是**条件性**失效而非结构性。
真实约束是「H1 仅在对话框为前台时有效」。
教训:看到 `HWND(0x0)` 这个异常值就死盯它,忽略了 48 次成功这个更大的样本。
**定性任何观测值之前,必须先看全量分布,而不是最近 N 行。**

完整教训见 `2026-10-07-focus-21-response-to-claude-v0.5.6-plan.md` §8。

---

## 部署与运维契约:计划任务生命周期

> **用户明确要求(2026-10-07)**
> 1. **安装软件时自动建立**计划任务
> 2. **卸载软件时自动删除**计划任务
>
> **实测状态:该契约早已实现,本次逐项复核并重建了任务。**

### 涉及脚本

| 脚本 | 职责 | 幂等性 |
|---|---|---|
| `register-scheduled-task.ps1` | 创建任务;若已有任务指向**不同**二进制路径,先注销旧任务再重建 | ✅ 可重复执行 |
| `unregister-scheduled-task.ps1` | 删除任务 + 清理孤儿 sidecar 进程 | ✅ 任务已不存在时 exit 0 |

### 安装器接线

```
installer.nsi:165   File "unregister-scheduled-task.ps1"        ← 打包进安装目录
installer.nsi:194   nsExec … register-scheduled-task.ps1        ← Stage 6.5 安装时建
installer.nsi:250   nsExec … unregister-scheduled-task.ps1      ← Uninstaller 删
```

### 任务规格

```
Name      : KizemoFocusSync
Trigger   : AtLogOn
Principal : LogonType=Interactive   RunLevel=Limited
Action    : <install-dir>\bin\focus-sync-sidecar.exe --service
Settings  : AllowStartIfOnBatteries / RestartCount 3 / ExecutionTimeLimit 0
```

`--service` 参数是必需的:没有它,sidecar 会用 retry-bind(800ms × 3),
在多用户机器上触发 Task Scheduler 重启风暴(见 register 脚本内 round 19g 注释)。

### 规范安装路径

```
%APPDATA%\com.sigma-file-manager.app\extensions\kizemo.focus-sync\
    bin\focus-sync-sidecar\focus-sync-sidecar.exe
```

与 `manual-install.ps1` 的 `$binDir` 一致,也与 `register.ps1` 里
`__binaries["focus-sync-sidecar"].path` 的 canonical 分支一致。

### 本次重建记录(2026-10-07 15:41)

因调试需要曾手动运行 sidecar,任务被删除。已用**安装器自带脚本**重建,
保证与正式安装行为一致:

```
binary → %APPDATA%\...\kizemo.focus-sync\bin\focus-sync-sidecar\focus-sync-sidecar.exe
         sha 1330197F54D3E9A893B68B1D0501302AEEFE8A2529AA891DAECCB5D4DDF45EE1
task   → State=Running, sidecar PID 12904, mode=service, /health 正常
```

> ⚠️ **端口冲突陷阱**:若已有 sidecar 占用 37421,任务的 `--service`(fail-fast)
> 模式会直接启动失败。更换二进制前必须先停掉正在运行的 sidecar。

### 验证方法

```powershell
# 任务存在且指向正确路径
Get-ScheduledTask -TaskName 'KizemoFocusSync' |
    Select-Object TaskName, State, @{n='Exe';e={$_.Actions[0].Execute}}

# 卸载路径是否接线(应看到 register 与 unregister 各一处)
Select-String -Path release\extension-installer\installer.nsi `
              -Pattern 'register-scheduled-task|unregister-scheduled-task'
```
