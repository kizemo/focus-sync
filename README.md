# Focus Sync

**Listary 风格的全局焦点同步 —— 让 Save As / 下载弹窗自动跟随你在 Sigma FM 里切换的目录。**

你在 Sigma FM 里切换目录 → 所有已打开的文件对话框自动跳到同一个目录。

---

## ⚠️ 仓库边界(先读这一段)

本仓库 **只包含 Focus Sync 本身**,不包含 Sigma FM。

| | 位置 | 说明 |
|---|---|---|
| **Focus Sync**(本仓库) | `sigma-listary-spike/` `release/extension/` `release/extension-installer/` | sidecar Rust 源码 + 插件本体 + NSIS 安装器 |
| **Sigma FM fork** | [`github.com/kizemo/sigma-file-manager`](https://github.com/kizemo/sigma-file-manager) | 独立仓库,fork 自 [`aleksey-hoffman/sigma-file-manager`](https://github.com/aleksey-hoffman/sigma-file-manager)。本仓库通过 `.gitignore` 完全排除它 |

**两者零源码耦合。** 本项目只通过 Sigma FM 的**公开扩展 API**(manifest / 沙箱 / 权限)与之交互。
最直接的证据:沙箱规则由 [`scripts/scan-sandbox-dynamic.cjs`](scripts/scan-sandbox-dynamic.cjs)
**在运行时从 fork 的 `sandbox.ts` 提取**,而不是把沙箱代码复制过来 —— fork 更新,门禁自动跟上。

> 仓库曾名为 `filemanager`,但其中并无文件管理器。改名以消除误导。

---

## 架构

```
┌─────────────────────────────┐
│  Sigma FM (独立仓库,fork)   │
│  └─ extension worker        │
│      kizemo.focus-sync       │
└───────────┬─────────────────┘
            │ ① onPathChange 事件
            │ ② POST /set_path
            ▼
┌─────────────────────────────┐
│  sidecar (Rust, 127.0.0.1:37421)
│  ├─ HTTP server  收路径      │
│  ├─ UIA monitor  只认前台对话框│
│  └─ writer      注入对话框    │
└─────────────────────────────┘
            │ ③ UIA / SendInput
            ▼
   Save As / 下载弹窗(地址栏)
```

### 为什么需要 sidecar

Sigma FM 的 extension 跑在 **Web Worker** 里,拿不到桌面窗口句柄,无法驱动
其他进程的文件对话框。需要一个同会话的本地进程做 UIA 注入。

sidecar 由 **Windows 计划任务 `KizemoFocusSync`** 启动(登录时),与 Sigma FM 生命周期解耦。

### 关键设计约束

| 约束 | 原因 |
|---|---|
| **绝不抢焦点** | 用户在 Sigma FM 操作时,sidecar 不得把焦点切回对话框。前台状态由 monitor 线程(有 Windows 消息泵)发布到共享 `AppState`,HTTP 线程**不自行查询** `GetForegroundWindow()`(无消息泵时返回值不可靠) |
| **非前台对话框只延后,不写入** | 用户手动切回对话框时才同步 |
| **地址栏而非文件名框** | 往文件名框写路径只是污染,不是导航。WinUI 3 / Chromium 对话框**没有可 SetValue 的地址栏元素**,只能用 `Ctrl+L` |
| **两道护栏** | ① 文件名框读回比对 ② `GetGUIThreadInfo` 实时焦点检查。任一不通过就**拒绝按 Enter**,失败方向是「不生效」而非「误保存」 |

---

## 组件

| 组件 | 位置 | 规模 | 说明 |
|---|---|---|---|
| sidecar | `sigma-listary-spike/` | 13 文件 / ~3.9k 行 Rust | crate `sigma-listary-spike`,产物名 `spike` |
| 插件 | `release/extension/` | `dist/index.js` ~686 行 | ESM,`onStartup` 激活 |
| 安装器 | `release/extension-installer/` | NSIS + 10 个 PS 脚本 | 计划任务的建/删 |
| 工具链 | `scripts/` | 5 个脚本 | 沙箱扫描 + 日志读取 |

### 插件权限(最小集)

```json
["commands", "toolbar", { "name": "http", "hosts": ["http://127.0.0.1:37421"] }, "notifications"]
```

只允许访问本机 sidecar,**无 `shell`、无 `fs`**。

---

## 安装

### 方式一:NSIS 安装器(推荐)

```powershell
release\extension-installer\build.ps1        # 打包
# 运行 kizemo.focus-sync-0.2.0-setup.exe
```

安装器会:
1. 复制插件到 `%APPDATA%\com.sigma-file-manager.app\extensions\kizemo.focus-sync\`
2. 写 `user-extensions.json`(`isLocal: true`,防止被自动清理)
3. 注册计划任务 `KizemoFocusSync`(AtLogOn / Interactive / Limited)
4. 把 sidecar 装到规范路径

**卸载时自动删除计划任务并清理孤儿进程。**

### 方式二:手动部署(调试用)

```powershell
# 1. 构建并部署 sidecar 到规范路径
.\sigma-file-manager\scripts\build-with-sidecar.ps1 -RepoRoot $PWD

# 2. 复制插件文件
Copy-Item release\extension\dist\index.js `
  "$env:APPDATA\com.sigma-file-manager.app\extensions\kizemo.focus-sync\dist\index.js" -Force

# 3. 注册计划任务
.\release\extension-installer\register-scheduled-task.ps1 -SidecarPath <sidecar.exe>

# 4. 卸载时
.\release\extension-installer\unregister-scheduled-task.ps1
```

> **端口冲突陷阱**:换 sidecar 二进制前**必须先停掉正在运行的实例**。
> 计划任务用 `--service`(fail-fast)模式,端口被占会直接启动失败。

---

## 开发

### 构建 sidecar

```powershell
$env:PATH = "C:\Users\Duanyi\.rustup\toolchains\1.85.0-x86_64-pc-windows-msvc\bin;$env:PATH"
cd sigma-listary-spike
cargo build --release
```

### 修改插件后必须跑沙箱门禁

```powershell
node scripts\scan-sandbox-dynamic.cjs release\extension\dist\index.js
```

**期望:`VALID (0 violations / 19 patterns)`**

⚠️ **沙箱会扫描注释,不只扫描代码。** 一句 `foreground window.` 曾经导致 extension
被静默拒绝、永不加载(sidecar 108 分钟 0 请求),且没有任何 UI 报错。

规则从 `sandbox.ts` **实时提取**(不是硬编码快照)—— 该文件在 git 历史中变更过
(`ba3e5a3f` 新增 `.constructor()`),硬编码会静默失效。

**门禁已长在部署路径上:** 构建 / 部署 / 注册 / commit 四道 Hook 都会自动跑它。

---

## 诊断工具

出问题时的标准动作 —— **读持久化日志,不要只看 DevTools**。

### 读 sidecar 日志

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\read-sidecar-log.ps1
```

- 同时呈现 `spike.log`(JSONL 事件)与 `spike.log.YYYY-MM-DD`(tracing 原因)
- **强制 UTF-8 解码**(sidecar 响应不带 charset,PowerShell 默认 ANSI 会造成假乱码)
- 枚举当前存活的 `#32770` 对话框并与 `active_dialogs` 对比 → **一眼看出注册表里是不是死句柄**

### 读插件 trace

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\read-focus-trace.ps1
```

插件把节点级 trace(`N01`–`N15`)持久化到 `sigma.storage`,**跨重启保留**。
编码判据存成**码元数值**而非字符串 —— 正在追查乱码 bug 时,乱码本身不能污染证据。

### 日志位置

```
%LOCALAPPDATA%\kizemo\focus-sync\logs\spike.log              JSONL 事件
%LOCALAPPDATA%\kizemo\focus-sync\logs\spike.log.YYYY-MM-DD   tracing 原因
```

> 只看第一个文件会**永远看不到失败的真实原因** —— 它在第二个文件里。

---

## 已知限制

| 项 | 影响 |
|---|---|
| **死 HWND 累积** | 对话框关闭后注册表仍保留条目,每次写入产生一次假的 `unsupported_dialog_type`(污染 `unsupported_dialog_count`) |
| **H2 假成功** | 回退路径 H2 只表示两次 `SetValue` 都执行了,**不表示路径真的生效** |
| **`getCurrentPath()` 返回 null** | 插件除 `onPathChange` 外无第二条获取当前路径的途径 |
| **WinUI 3 对话框** | 无法通过 COM `IFileDialog::SetFolder` 导航(结构性的)。Edge 可用 `edge://flags/#edge-legacy-file-picker` 切回经典选择器 |
| **推送链路未完全可观测** | 已定位待查项,见 `docs/superpowers/plans/` |

---

## 文档

| 文档 | 内容 |
|---|---|
| [`docs/extension-changelog.md`](docs/extension-changelog.md) | **v0.3.0 → v0.5.5 完整变更史 + 沙箱约束 + 部署契约** |
| `docs/superpowers/plans/2026-10-08-focus-21-full-retrospective.md` | 完整复盘:18 小时排障的错误清单与方法论沉淀 |
| `docs/superpowers/plans/` | 设计与实施计划存档 |

> **变更历史不要写回 `index.js`** —— 注释同样会被沙箱扫描。它属于文档。

---

## License

MIT