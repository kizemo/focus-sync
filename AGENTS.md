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
- **需要装机实测就直接打包 exe，不要等用户开口**（用户 2026-10-19 明确要求）。判定条件是「**结论需要真机才能成立**」：新功能缺界面证据、缺陷修复只有截图能确认、改了安装器/打包脚本（NSIS 钩子、内嵌资源、单实例守卫）——这类不要只交代码 + 门禁数字就把「请用户装包」写成下一步。命令用 `npm run tauri:build`（末尾的 `publish-bundle-to-release.mjs` **只把安装包复制到仓库 `release/`，不上传**，可以放心跑）。`tauri build` 很慢（Rust release + 打包），所以**一旦判断出要真机验收就早启动、丢后台跑**，再做文档、补测试等不占构建锁的活。构建脚本放 `.superpowers/sdd/*.cmd` 且**必须纯 ASCII**（cmd.exe 按系统 ANSI 代码页读 `.cmd`，中文注释会变乱码并被当成命令执行）。
- 拿不到 webview console 是本项目的常态：wry 不透传 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`，CDP 连不上。可用的取证手段是 `PrintWindow(hwnd, hdc, 2)` 截图、UIA 树枚举（`InvokePattern` 可点击，Tauri 窗口 class 为 `Tauri Window`），以及页面内 Vue DevTools 的组件搜索框（UIA `ValuePattern` 可写入）。UIA 判别时要带对照项，别只搜一个名字。
- **`SendInput` 其实能进 WebView2 —— 此前「SendInput 对 WebView2 无效」的结论是错的，已作废**（2026-10-15 21:5x 复核）：`.superpowers/sdd/app-key.ps1` 的 40 字节 `INPUT` 结构、`SetForegroundWindow`、`Marshal.SizeOf=40` 都是对的，`SendInput` 每次都返回 1，Ctrl+V 真的把剪贴板内容粘进了工具栏过滤框并触发了过滤。真凶是**那个脚本的字面量解析**：非组合键 token 会走 `[Convert]::ToUInt32(char.ToUpper(), 16)`，即把字母当十六进制解析，于是 `V` / `W` / `I` 这些非 A–F 的字母直接抛 `FormatException`，脚本中途退出 —— 看起来就像「键打不进去」。已把 A–Z 的 VK 补进 `$named` 表。
  由此得到输入文字的**首选路径**：`Set-Clipboard` + `app-key.ps1 -Keys 'CTRL+V'`（中文/emoji 同样免疫 IME）。**失败的替代方案，别再试**：往 `Chrome_RenderWidgetHostHWND` 投 `WM_CHAR`（`0x0102`）会被静默丢弃；剪贴板 + 中键粘贴（`WM_MBUTTONDOWN/UP`）也不会触发粘贴。
- **本应用的工具栏按钮不暴露 UIA `InvokePattern`**（2026-10-15 实测）：文件浏览器工具栏的过滤按钮探出来是 `[Button] 55x54 pats=`，空 pattern。**可用的第二条路**：`PostMessage(WM_LBUTTONDOWN=0x0201 / WM_LBUTTONUP=0x0202)` 发给 **`Chrome_RenderWidgetHostHWND`**（不是 `Chrome_WidgetWin_0`），`lParam` 用**该窗口客户区坐标**（屏幕坐标减去它的原点）。单击会被应用收到（grid 卡片会选中、过滤框会打开）。键盘则直接走上面的 `SendInput`。
  **过滤框本身在 UIA 树里是个空壳**：打开后只多出一个 `[Window] name="" 510x56 @(664,82)`（Chromium 给可编辑区建的原生 IME 窗口），下面挂的是 `-∞x-∞` 的 `[Group]`，**没有 `[Edit]`、没有 `ValuePattern`**。所以「用 UIA SetValue 写过滤词」这条路在本项目不存在，只能靠键盘注入。
  **写注入探针时必须带阳性对照**：先点一个必然有可见后果的元素（文件夹卡片 → 出现选中态），确认内容哈希变了，再去点目标按钮。否则「没反应」分不清是注入失败还是功能本身没反应。
  全局搜索的放大镜（标题栏 `(1450,38)`）与右键菜单项（Chromium accessibility 的 `MenuItem` Name）**有** `InvokePattern`，可以直接调。
- **验收夹具不要建在 `%TEMP%` 下**（2026-10-15 实测）：`src-tauri/src/global_search/ignore.rs` 的 `builtin_ignored_paths()` 含 `/AppData/Local/Temp`，且 `matches_plain_ignored_path` 对 `/` 开头的模式做 `path.contains(...)`。夹具放 `%TEMP%` 时整棵子树被剪掉，**目录内递归搜索的命中恒为 0** —— 看起来像产品缺陷，其实是被忽略规则挡死了。放到 `C:\Users\<user>\` 下的普通目录即可。
- **开关型控件要先探测再点击**（2026-10-15 实测）：过滤按钮是 toggle，上一轮脚本点了之后输入框还开着，下一轮再点就把它关了，于是得到一条假的「找不到输入框」。脚本里先查目标元素是否存在，不存在才点。
- **UIA 里从点往上找祖先会一路走到窗口级元素**（2026-10-15 实测）：`TAURI_DRAG_RESIZE_WINDOW` 覆盖整个客户区，取它的中心点右键 = 点在「搜索面板与导航器面板的缝」上，弹出的是导航器的菜单而不是搜索结果的菜单。必须给祖先加硬边界（宽度上限 + 落在目标区域内），不能只看「有 Name」。
- Chromium 把列表行暴露成什么 `ControlType` **不稳定**（2026-10-15 实测）：全局搜索结果行**不是** `ListItem`，`AutomationElement.FromPoint` 也没给出可用祖先。稳定的是行内那个 **28×28 的图标**（面板其它图标是 19×19 / 25×25），按尺寸挑第一个即可定位整行。
- **不要用 `Process.MainWindowHandle` 取本项目的主窗口**（2026-10-15 实测）：它会落在 15×15 的托盘窗 `com.sigma-file-manager.app-sic` 上；进程里还有隐藏的 `Alpha File Manager | Quick View` 与 `tray_icon_app` / `global_hotkey_app`。要自己 `EnumWindows` + `GetWindowThreadProcessId` 过滤，再取 **class 为 `Tauri Window`、可见、面积最大**的那个。
- **改 `user-settings.json` 的 `navigator.defaultDirectory` 不等于应用会打开那个目录**（2026-10-15 实测）：`user-workspaces.json` 里保存的当前标签页优先，会把启动目录钉死在原处。验收要量指定目录时，必须同时改掉 `workspaces[].tabGroups[][]` 中 `type === 'directory'` 的 `path`，否则量到的是另一个目录。
- 验收量到的是**物理像素**：本机 DPI 144（150% 缩放），换算 CSS 逻辑像素要除以 1.5。2026-10-15 那一轮就是因为忘了除，把目录卡的 `78` 读成「小档应为 52，不符」而差点误判成产品 bug。
- `PrintWindow(hwnd, hdc, 2)` 在大列表**平滑滚动动画未结束时**会抓到未绘制的黑块。遇到疑似渲染缺陷，先延长等待并连拍两次取后一帧再下结论——别把抓屏时机问题当成产品缺陷上报。
- `PrintWindow` **不渲染 hover / overlay 层**（悬停才出现的快捷面板、右键菜单都不在里面）。验这类交互只能走**屏幕 DC BitBlt**，即拍屏幕合成结果。
- 屏幕 BitBlt 只拍屏幕合成结果：**别的窗口盖在上面就会拍到别的窗口**（此前 Edge 盖住应用导致拍到 GitHub 页面，产出两次假的「点击没反应」）。用 `WindowFromPoint` + `GetAncestor(h, 2)` 校验实际绘制者。
  - **但白名单必须包含应用自己的 `msedgewebview2`**（2026-10-15 实测）：webview2 的 `Chrome_WidgetWin_0` 顶层窗口就是光标落在页面区域时下方的那个窗口，`owner pid` 会是 `msedgewebview2.exe` 而不是主进程。只认主进程 pid 会把**每一张干净截图**都判成「被遮挡」而中止。正确做法是放行主进程 + 父进程链能回到主进程的全部 `msedgewebview2`。
- **悬停面板里的坐标不能硬编码**（2026-10-15 实测）：面板是 hover 驱动的，滚动位置在多次运行之间会漂（点树的滚轮落点与面板重叠时尤其明显），写死的表头 Y 会落到列表项上而不报错。要**在点击前从当帧实测字形位置**，按序号选中第 N 个，而不是按写死的 Y。
  - 面板 x 范围约 `70..480`，目录树 x 范围约 `80..340`，**两者重叠**。在导航器路由上想把面板滚到顶，滚轮落点必须避开目录树（用 `x≈455`），否则滚的是树。
- 验收脚本里 **PowerShell 函数只要有返回值，就不能在里面混用 `Write-Output`**：诊断输出会进管道并成为返回值的一部分，实测让点击坐标变成字符串、脚本在第一条断言前静默退出。函数内改用 `Write-Host`。
- **门禁基线必须用同一条命令量，混口径会误判成回归**（2026-10-15 实测）：项目 npm 脚本是 `eslint .`（整仓），基线数字 `186 errors / 121 warnings` 就是这个口径；改成 `eslint src` 在同一个基线提交（`0f73f159`）上只有 `179 / 121`——差的 7 个是 `src` 之外的存量。拿 `eslint src` 的数字去对 186，会凭空多出 7 个「回归」。
  稳妥做法是在基线建一个 worktree 跑**同一条命令**做逐字对照：
  `git worktree add --detach $env:TEMP\afm-baseline-eslint 0f73f159`（该目录已有 `node_modules`）。
- **`eslint .` 的数字必须是「干净检出」那个数**（2026-10-21 实测）：eslint **不读 `.gitignore`**，
  所以工作副本里的 `.superpowers/`（150 多个一次性 `.cjs` 脚本，`.superpowers/sdd/.gitignore` 是 `*`，
  整个目录不进版本库）会被一起 lint —— 同一个提交，工作副本里量到 **689 errors**，干净检出里只有 **177**。
  已把 `.superpowers/**` 与 `graphify-out/**` 写进 `eslint.config.ts` 的 `globalIgnores`
  （与已有的 `_acceptance/**` 同理），`eslint .` 现在稳定在 **177 errors / 0 warnings**
  （= `eslint src` 的 170 + `eslint scripts` 的 7）。
  **看到几百个错误先按目录分组看**，别急着当成回归 —— 先分清「仓库里真有的」和「只有工作副本才有的」。
- **单文件 `eslint --fix` 通过 ≠ 回到基线**（2026-10-15 实测）：`--fix` 会把该文件里**基线本来就有的**违规一起修掉，单文件复验会显示 0 个问题，但总数可能已经低于或高于基线。必须再跑一次全量对照才算数。
- `$ErrorActionPreference = 'Stop'` 会把**原生命令的 stderr**变成终止错误（`NativeCommandError`），门禁脚本在调 `npm` / `npx` 前后必须显式切回 `Continue`，否则第一条 warning 输出的 stderr 就让脚本带 exit 1 退出，看起来像门禁挂了。
- `vitest --reporter=basic` 在本仓库这版不存在，会 `Failed to load custom Reporter from basic` 直接中止。跑全量单测用默认 reporter，再按 `Test Files` / `Tests` 两行抓结果。
- **打包的 MSVC/Rust 注入**（每次 bash 调用都要重来）：`cargo` 不在 PATH，在 `C:\Users\Duanyi\.rustup\toolchains\1.90.0-x86_64-pc-windows-msvc\bin`；MSVC 环境靠 `call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"`（该路径存在，MSVC 工具版本 `14.44.35207`）。可直接复用 `.superpowers/sdd/build-fb4.cmd`。
- **绝不要用 PowerShell 的 `Get-Content -Raw` / `Set-Content` 改带中文注释的源文件**（2026-10-15 踩到，代价是一个文件的中文全变 mojibake）：PowerShell 5.1 的 `Get-Content` 默认按 **ANSI** 读无 BOM 的 UTF-8，中文注释读进来就是乱码，再写回去就把文件毁了。`Set-Content -Encoding UTF8` 还会额外写 BOM。
  要改仓库里的 `.vue` / `.ts` / `.json`，一律用**编辑工具**，或 `git checkout -- <file>` 恢复后重做；批量改 JSON 用 `node` 的 `fs.readFileSync/writeFileSync(..., 'utf8')`。
  改完立刻用 node 验一遍：`s.includes('某个中文注释')` 且 `s.match(/\uFFFD/g)` 为空，再往下走。
- **给 `user-stats` 加顶层字段，「读白名单」和「写列表」要一起改**（2026-10-15 真机踩到）：读侧 `USER_STATS_TOP_LEVEL_KEYS = Object.keys(DEFAULT_USER_STATS)`，把字段加进 `DEFAULT_USER_STATS` 就自动放行；写侧 `saveStats()` 却是**逐键显式 `set()`** 的固定五项。漏掉写侧的后果是：新建收藏分类 → 界面出现 → 下拉里也有 → **重启即丢**，`user-stats.json` 里该键是 `undefined`，全程不报错。已修（`saveStats` 补 `favoriteCategories`）。
  **只断言内存状态的单测抓不到这个**——`store.favoriteCategories` 一直是对的。要断言 `storedValues.get('favoriteCategories')` 这种真正走一遍 mock store `entries()` 的往返。
- **`InvokePattern.Invoke()` 返回成功 ≠ 点击生效**（2026-10-15 实测）：对**禁用**的按钮调 Invoke 是静默空操作，脚本照样打印 `invoked=yes`。「按钮有没有真的生效」只能靠**落盘文件**或**像素可见变化**判定。
  同理 UIA `ValuePattern.SetValue` 的 read-back 只说明值进了 DOM 输入框；**Vue 的 `v-model` 是否响应要单独验**（本项目实测会触发 `input`，可用「填一个不存在的路径，看校验提示是否出现」当阳性对照）。
- **`user-stats.json` 是验收反馈 4 的事实来源**：仪表盘的分类/收藏状态都能从这里读到，比看界面更硬。路径 `C:\Users\Duanyi\AppData\Roaming\com.sigma-file-manager.app\user-data\user-stats.json`。
- **`cargo test --lib` 的测试二进制里有一条隐形红线：任何被测试链接到的代码都不能碰到 `tauri::AppHandle` 或 `tauri::async_runtime::*`**（2026-10-16 踩到，代价是一整轮无法定位的排查）。
  现象：测试二进制在**加载阶段**以 `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139) 崩掉，一个测试都跑不了，报错里完全看不出跟你的模块有关。
  根因三步：① 本仓 `lib.rs` 的 `.run()` 持有全部 command，但测试不调 `run()`，所以基线上 **tao 一条导入都没有**，GUI 代码整段被链接器丢掉；② 一旦你的代码被测试链接到并碰到 `tauri::async_runtime::spawn_blocking`，它的 `static RUNTIME: OnceLock<GlobalRuntime<Wry>>` **带默认类型参数 `Wry`**，一处调用就单态化出 `<Wry as Runtime>`，把 tao 整条窗口链拖进来（实测新增 152 个导入，user32 占 104 个）；③ tao 里有 `comctl32.dll!TaskDialogIndirect`，它属于 **comctl32 v6**，只有 application manifest 声明了才解析得到，测试二进制没有 manifest → 加载器找不到导出。
  定位手段：`dumpbin /DEPENDENTS` 看新增 DLL → 二分 `mod` 声明缩小范围 → `dumpbin /IMPORTS` 与基线二进制**逐项 diff 导入表**。⚠️ 基线必须是 `git stash` 后**现重建**的那份，`target/debug/deps` 里的历史产物会给出「新增 152 个 tao 导入」这种假结论。
  绕法：给业务模块加 `*_blocking` 同步内核入口（见 `src-tauri/src/dir_reader.rs`、`src-tauri/src/global_search.rs`），调用方自己选执行器。
- **`#[tool_router]` 不等于接线完成**（2026-10-16，rmcp）：只写 `#[tool_router] impl X {…}` 加自写的 `impl ServerHandler for X { fn get_info }`，编译通过、单测通过，**只有 `tools/list` 返回 `{"tools":[]}`**，一个工具都调不到。必须给 `impl ServerHandler` 加 `#[tool_handler]`。rustc 对 `tool_router` 字段报的 `never read` 警告在这里**不是误报**，是线索。
- **验收时会有置顶的 Electron 窗口盖住应用**（2026-10-15）：遮挡守卫会正确地 `ABORT`，但 `ShowWindow(SW_MINIMIZE)` 对它无效（会自己弹回）。要把它 `MoveWindow` 到 `(-4000,-4000)`，并把原 rect 记进 `occluders.json` 以便收尾复原（`.superpowers/sdd/shift-occluders.ps1`）。

## 3. 代码导航：graphify 知识图谱

本项目的**内层仓库** `sigma-file-manager/` 已建好本地 AST 知识图谱（2026-10-21 部署）：**零 LLM、零 API 成本、纯本地**，产物在 `sigma-file-manager/graphify-out/`（已加进 `.gitignore`，不进版本库）。

- **重建**：`cd sigma-file-manager && graphify extract . --code-only`（1243 个代码文件 / 21 万行，**约 42 秒**）；改过代码就 `graphify update .`。
- **当前规模**：16548 节点 / 34722 边 / 723 社区，96% 边为 `EXTRACTED`、4% `INFERRED`。
- **查问题**：`graphify query "..."`（返回相关子图）、`graphify path "A" "B"`（最短路径）、`graphify explain "X"`（单个符号的全部连接）、`graphify affected "模块名" --depth 2`（**改这里会波及谁**）、`graphify god-nodes --top 10`（核心抽象）。
  MCP 侧工具同名（`query_graph` / `shortest_path` / `get_node` / `god_nodes` / `graph_stats`），MiniMax Code 与 cc-haha 都已注册 `graphify` 这个 MCP server；这些工具都吃可选的 `project_path`（指向含 `graphify-out/graph.json` 的目录），所以**一个注册服务所有项目**。
- `graphify affected "drag-no-activate.ts"` 实测能一次列全该文件的调用方、导入方与 3 个相关测试文件——「改这里影响谁」这种问题 grep 给不出完整答案，这是图最实用的场景。

**已知盲区（别据此下结论，每条都是实测）**：

- **Rust 侧不抓 `mod` 声明**。抽取器抓 `use` 语句（→`imports_from`）和函数定义（→`contains`），但不抓 `mod foo;`。实测 `src-tauri/src/lib.rs` 的文件节点**出边为 0**，而 `lib.rs:20` 有 `mod drag_no_activate;`、`lib.rs:332-341` 注册了 9 个 command。所以对 Rust 模块跑 `graphify affected` 会返回「No affected nodes found」，**这不代表没人依赖它**，只代表图没建这条边。
- **Tauri IPC 边界是断的**。TS 侧 `invoke('set_drag_no_activate')` 的字符串实参**不产生边**；Rust 侧 `pub fn set_drag_no_activate()` 虽然作为符号在图里，但两边互不相连。跨语言追踪必须手工搭桥：以 **command 名字**为锚点，先在 Rust 侧 `graphify query "<command>"` 找定义，再在 TS 侧搜 `invoke('<command>')`。
- **AST 缓存命中率只有约 25%**（1243 个文件只缓存了 309 个），所以 `update` 与全量 `extract` 耗时接近（32–54 秒），**别指望 `update` 是秒级**。
- **节点/社区数在两次运行间会变**（12814 → 16548，因为图里混入了文档概念节点）。**不要把绝对数当稳定指标，更不要拿它做回归门禁。**
- **同名节点会匹配错**。实测 `graphify explain "lib.rs"` 命中的是 TS 测试文件里的 Rust 源码字符串 `LIB_RS`，不是 `src-tauri/src/lib.rs`。名字有歧义时改用 `graphify query` 看候选，或用 MCP `get_node` 传精确 node id。
- **`INFERRED` 边要人工复核**。图自己会在报告的 "Suggested Questions" 里追问「这些推断出来的关系对不对」；实测已出现把 ESLint 配置注释误判成引用 `DirEntry` 的 INFERRED 边。