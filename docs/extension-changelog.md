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
| **H1 只在对话框打开时有效** | **开放,待修复** | 全量日志统计:`H1-OK 48 / H1-FAIL 8`,**无一例外** —— 对话框刚打开(`opened_write_back`)时前台是它,H1 成功并真正导航;用户在 Sigma FM 里切目录(`/set_path`)时前台是 Sigma FM,H1 必然失败并退到 H2。H2 往 filename 控件写路径再还原,**按 Claude Rule 58 那不是导航** => 用户抱怨的是「打开弹窗时对、之后切目录就不动」 |
| **COM SetFolder 从未成功** | 结构性 | 全部日志 `COM SetFolder succeeded = 0 / unavailable = 107`。用户的对话框一直是 WinUI 3 封装,拿不到经典 IFileDialog。**零代码验证法**:开 `edge://flags/#edge-legacy-file-picker` 后重测,若立刻出现 succeeded,则 H1/H2/F3 回退链可退休 |
| **缺陷 B:告警通道整体失效** | **开放** | `JSON.parse(/health body)` 失败 => `checkHealth()` 的 `unsupported_dialog_count` 整块被跳过 =>「对话框不受支持」提示**永不触发**。已加 `N12/N13/N14` 节点,重启后 15 秒内可得响应体指纹 |
| **缺陷 A:对死 HWND 写入** | **开放** | 对话框关闭后注册表仍保留条目,写入返回 `0x80040201` 并被误标为 `unsupported_dialog_type`,污染 `unsupported_dialog_count` |
| **`/health` 响应缺 `charset`** | **开放** | `Content-Type: application/json` 未带 `charset=utf-8`,任何非 UTF-8 默认代码页的客户端都会看到乱码。**2026-10-07 曾因此导致一次假根因调查** |
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
