# 项目工作约定

本文件是本仓库的 Agent 约定。后续会话在改动本项目代码前必须先读本文件。

## 1. 输出语言

**面向用户的输出一律使用简体中文。**

具体包括：

- 对话回复、进度说明、结论与汇报
- 交接文档（`docs/handoff-*.md`）、计划与评审文档
- 应用界面文案与 17 个语言包（`src/localization/messages/**`，中文的 locale code 是 `ch` 而非 `zh`）中的中文值
- 面向使用者的说明性文字（README、操作指引、提示信息）

例外，以下内容保持原样、不得翻译或改写：

- 代码标识符：变量名、函数名、文件路径、命令行、配置键名
- 技术专有名词本身（Pinia、Vitest、NSIS、sha256 等），在句子中可保留英文
- 代码注释中的标识符引用

判断标准：如果这句话是给人看的，就用简体中文；如果是机器要读的，就保持原样。

## 2. 本项目的其他硬约束

- 品牌名统一为 **Alpha File Manager**，相关决策已全部锁定，不要重新讨论命名或图标。
- 受保护常量不得改动：mDNS 服务名、legacy 数据目录名、Tauri `identifier`。
- 验证结论前先确认观察窗口足够长（本项目扩展冷启动需 3–5 分钟）。
- **真机验收必须用 `tauri build` 的打包产物，不能用 `tauri dev`**。`tauri dev` 构建下应用外壳正常（左侧导航、盘符、状态栏都在），但**所有路由页面的内容区都是空白**——`RouterView` 已挂载却不渲染任何子组件（Vue DevTools 里搜 `navigator` / `settings` 只命中 Pinia store，搜 `sidebar` 能命中组件，可据此判别）。2026-10-12 已归因：该现象把 `src/` 全部回退到改动前仍然复现，而同机已安装的打包版完全正常，因此是 dev 构建特有，**不是代码缺陷**。在 dev 构建上做的界面验收结论一律作废。
- 拿不到 webview console 是本项目的常态：wry 不透传 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`，CDP 连不上；`SendInput` 不可用。可用的取证手段是 `PrintWindow(hwnd, hdc, 2)` 截图、UIA 树枚举（`InvokePattern` 可点击，Tauri 窗口 class 为 `Tauri Window`），以及页面内 Vue DevTools 的组件搜索框（UIA `ValuePattern` 可写入）。UIA 判别时要带对照项，别只搜一个名字。