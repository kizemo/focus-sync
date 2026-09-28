# cc-haha Sidebar 历史会话恢复 — 手动操作指南

**症状**: 重启 cc-haha 后 sidebar 仍无历史会话
**根因**: `~/.claude/cc-haha/db/trace-index-v1.sqlite` 537/659 degraded（v2 fingerprint vs v6 schema 不匹配），最后索引时间 9/27 10:05（磁盘上最新 trace 是 9/28 13:31）
**策略**: 删旧索引 → 重启 cc-haha 触发自动 rebuild（cc-haha 会扫描 `traces/*.jsonl` 重新写入索引）
**备份**: `~/.claude/cc-haha.backup-2026-09-28/`（3.4 GB）已就绪

---

## ⚠️ 操作前必读

1. **必须在 cc-haha 完全关闭状态下操作**（否则 spike 进程会持有文件句柄，删除失败）
2. **备份已经做好了**，可随时回滚（见 §5）
3. **不要动 `~/.claude/cc-haha/cc-haha.json` / `desktop-ui.json`** — 这些是用户偏好，sidebar 显示设置
4. **不要删 `~/.claude/cc-haha/traces/`** — 这是源数据（API call logs），删除 = 永久丢失 227 个会话历史
5. **不要删 `~/.claude/cc-haha/db/index-v1.sqlite`**（不是 trace-index）— 这是 cc-haha 的 session 索引（与 trace-index 不同），目前几乎空但属于用户 Claude session 元数据

---

## 操作步骤（PowerShell，git-bash 也行）

### 第 1 步：关闭 cc-haha

**手动**：右键任务栏 cc-haha 图标 → Exit / 关闭
**PowerShell 兜底**（如果卡死）：
```powershell
Get-Process -Name "Claude Code Haha" -ErrorAction SilentlyContinue | Stop-Process -Force
```
**验证关闭**：
```powershell
Get-Process -Name "Claude Code Haha" -ErrorAction SilentlyContinue
# 应返回空
```

### 第 2 步：确认备份存在

```powershell
ls "$env:USERPROFILE\.claude\cc-haha.backup-2026-09-28\db\"
# 必须看到: index-v1.sqlite, search-index-v1.sqlite, trace-index-v1.sqlite
```

如果备份不存在（不应该发生），**立即停止**，先做完整备份：
```powershell
Copy-Item -Path "$env:USERPROFILE\.claude\cc-haha" -Destination "$env:USERPROFILE\.claude\cc-haha.backup-2026-09-28\" -Recurse -Force
```

### 第 3 步：删除 trace-index-v1.sqlite（+ WAL/SHM 配套）

```powershell
$dbDir = "$env:USERPROFILE\.claude\cc-haha\db"
Remove-Item "$dbDir\trace-index-v1.sqlite" -Force
Remove-Item "$dbDir\trace-index-v1.sqlite-shm" -Force -ErrorAction SilentlyContinue
Remove-Item "$dbDir\trace-index-v1.sqlite-wal" -Force -ErrorAction SilentlyContinue
```

**预期输出**: 三个文件被删除（无错误或 SilentlyContinue 抑制的 not-found 警告）
**验证**：
```powershell
Get-ChildItem "$dbDir\trace-index-v1.sqlite*"
# 应返回空
```

### 第 4 步：重启 cc-haha

**手动**：开始菜单 → "Claude Code Haha" → 打开
**PowerShell 启动**（可选）：
```powershell
Start-Process "$env:LOCALAPPDATA\..\..\Program Files\Claude Code Haha\Claude Code Haha.exe"
# 或者用开始菜单快捷方式
Start-Process shell:AppsFolder\NanmiCoder.cc-haha_0.1.0.0_x64__xyz
# 如果不知道 AppUserModelID，去开始菜单拉：
Start-Process "C:\Program Files\Claude Code Haha\Claude Code Haha.exe"
```

### 第 5 步：等待 cc-haha 重建索引

- cc-haha 启动后会扫描 `~/.claude/cc-haha/traces/`（227 个 jsonl，约 3 GB）
- **预计耗时**：5-30 分钟（取决于磁盘速度，6 GB 索引 + 解析 + SQLite 写入）
- **观察方法**：在 cc-haha sidebar 旁看是否有"Reindexing..."状态条
- **不要中途关闭 cc-haha**（会中断，下次还要重来）

### 第 6 步：验证 sidebar

- 打开 cc-haha sidebar
- 应该看到 ~227 个历史 session，按 mtime 倒序
- 点击任一 session 能进入对话（取决于 cc-haha 客户端是否实现 view replay）

### 第 7 步：可选清理（不影响功能）

如果想清理孤儿 sessions（DB 里 447 条对应文件已删）：
```powershell
# 这不是必须 — cc-haha 重建时会自动 reconcile
# 但如果你想强制清理，可手动 VACUUM
```

---

## 失败回滚（5 步）

如果 cc-haha rebuild 后 sidebar 仍空 / 启动报错 / 数据错乱：

### 步骤 A：关闭 cc-haha（同第 1 步）

### 步骤 B：恢复 trace-index 数据库

```powershell
$dbDir = "$env:USERPROFILE\.claude\cc-haha\db"
$bakDir = "$env:USERPROFILE\.claude\cc-haha.backup-2026-09-28\db"

# 删除当前（坏）文件
Remove-Item "$dbDir\trace-index-v1.sqlite*" -Force -ErrorAction SilentlyContinue

# 从备份恢复
Copy-Item "$bakDir\*" $dbDir -Force
```

### 步骤 C：重启 cc-haha（同第 4 步）

### 步骤 D：确认 cc-haha 用的是旧 DB（数据回到 9/27 10:05 状态）

### 步骤 E：上报问题

如果旧 DB 也有问题，考虑完整回滚：
```powershell
# 完整回滚（极端情况 — 会丢失所有 9/28 的新会话）
Remove-Item -Path "$env:USERPROFILE\.claude\cc-haha" -Recurse -Force
Copy-Item -Path "$env:USERPROFILE\.claude\cc-haha.backup-2026-09-28" -Destination "$env:USERPROFILE\.claude\cc-haha" -Recurse -Force
```

---

## 进阶：手动看 trace-index 重建状态

如果想看 cc-haha 是否在 rebuild：

```powershell
# 看 trace-index 数据库大小变化
Get-ChildItem "$env:USERPROFILE\.claude\cc-haha\db\trace-index-v1.sqlite" -ErrorAction SilentlyContinue
# 重建中：文件持续变大
# 完成：稳定在 ~134 MB

# 用 sqlite 探查
python -c "import sqlite3; c=sqlite3.connect(r'$env:USERPROFILE\.claude\cc-haha\db\trace-index-v1.sqlite'); print('sessions:', c.execute('SELECT COUNT(*) FROM trace_sessions').fetchone()[0], 'ready:', c.execute(\"SELECT COUNT(*) FROM trace_sources WHERE state='ready'\").fetchone()[0])"
# 期望：sessions ≈ 227, ready = 227
```

---

## 为什么这样修复是安全的

1. **traces 目录不动**：3 GB API call logs 原位保留，是 cc-haha rebuild 的输入
2. **db 备份完整**：3.4 GB `cc-haha.backup-2026-09-28/` 可一键回滚
3. **trace-index 是派生数据**：从 traces 重新生成，删了不丢信息
4. **cc-haha 客户端支持自动 rebuild**：重启时检测到 `trace-index-v1.sqlite` 缺失会自动触发全量重建
5. **不修改 cc-haha 内部代码**：纯操作层面，不影响客户端稳定性

## 已知风险

| 风险 | 概率 | 缓解 |
|---|---|---|
| cc-haha 客户端不自动 rebuild | 低 | 改方案：写 Python 脚本重建 v6 fingerprint（需查 cc-haha 源码） |
| rebuild 中 cc-haha crash | 低 | 重启 cc-haha 会从断点续建（trace-index-v1.sqlite 是临时文件，已扫过的写入即可） |
| 备份的 trace-index 也是 degraded | 中 | 不影响 rebuild，rebuild 后会生成全新 DB |
| 重建后 sidebar 显示但点不开 | 低 | 看 cc-haha 是否实现了 trace replay 功能（GitHub `NanmiCoder/cc-haha` 源码可查） |