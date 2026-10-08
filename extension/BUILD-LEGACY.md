# ⚠️ `extension/` 是废弃的 spawn 架构 —— 请勿构建

## 一句话

**`extension/` 不是当前插件的源码。真正的源码是 `release/extension/dist/index.js`,
它是手工维护的,不要用这里的 rollup 链去构建它。**

## 为什么

这两个目录**不是"新旧版本"关系,而是两条不同的设计路线**:

| | `extension/src/index.ts` | `release/extension/dist/index.js` |
|---|---|---|
| 架构 | **spawn 模式**(v0.3.0 之前) | **Scheduled Task 模式**(v0.5.5) |
| 谁启动 sidecar | extension 自己调 `sigma.binary.getPath()` 然后 spawn | Windows 计划任务 `KizemoFocusSync` |
| 关键符号 | `SIDECAR_BINARY_ID`、`SidecarPath`、`--port` | 无 `SIDECAR_BINARY_ID`;有 `trace` / `decodeHttpBody` / `first_push_received` |
| 规模 | 237 行 TS | 726 行 JS |
| 状态 | **已废弃** | **唯一真相源** |

## 危险

```bash
npm install && npm run build     # ← 会用废弃实现覆盖 dist
```

**丢掉的不只是最近的修复,而是 v0.3.0 → v0.5.5 的整条演进**,包括:

- Phase 4 sandbox 门禁修复(extension 曾静默不加载 108 分钟)
- 焦点归还用户(不再从 Sigma FM 抢焦点)
- 有两道护栏的地址栏导航
- focus-19/20/21 的全部行为

## 已加的防护

`package.json` 的 `build` / `watch` 脚本已改为调用 `scripts/refuse-legacy-build.mjs`,
它会直接报错退出。**不会再有人不小心覆盖掉可用的实现。**

## 怎么改插件

直接编辑 **`release/extension/dist/index.js`**,改完必须跑沙箱门禁:

```powershell
node scripts\scan-sandbox-dynamic.cjs release\extension\dist\index.js
```

## 未来要恢复 TypeScript 构建链

需要先把 `dist/index.js` 反向整理成 TypeScript(726 行,含类型标注),
让它等价于当前实现,然后才能让 rollup 接管。**这是一项独立的工程,必须单独立项做** ——
在此之前,手工维护 `dist` 是唯一安全的选择。

---

相关:[`../docs/extension-changelog.md`](../docs/extension-changelog.md)
(v0.3.0 → v0.5.5 完整变更史与沙箱约束)