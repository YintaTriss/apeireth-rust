# 前端交接文档（companion-desktop）

> 写给任何接手的人（人或 AI）。最后更新：2026-08-22，状态与代码 HEAD 对齐。
> 北极星：**电影感，不是软件感**。前端与后端浑然一体——用户住在一艘星环空间站的舰桥里，
> 舷窗外是行星、星环、远处的黑洞；Apeireth 的一切能力从这个场景里长出来。

## 0. 一句话现状

前端（Tauri 2 + Svelte 5 runes + TS，纯 WebGL2 场景引擎，零框架依赖）与后端
（`companion_serve`，:8090）**已端到端打通**：真实模型流式对话、真实工具审计、
真实健康诊断全部在跑。三模式骨架（陪伴/工程/专注）已验收。开场动画 v1 封存。

## 1. 怎么跑

```bash
# 后端（:8090，先起）
cargo build --example companion_serve -p apeireth-companion
# 运行需 env：APEIRETH_API_KEY（LLM key，主人在 C:\Users\31683\apikey-ultra.txt）、
# APEIRETH_MASTER_TOKEN（授权主令牌）。详见 docs/02-guides/quick-start.md。

# 前端（dev，:5199 是集群施工约定端口）
cd frontend/companion-desktop && pnpm dev --port 5199 --strictPort
# 质量门：pnpm check 必须 0 errors 0 warnings（svelte-check）
```

开发调试参数（URL query，全是已有纪律）：`?mode=focus|engineering`（初始模式）、
`?hour=<0-24>`（舰内时刻）、`?t0=<秒>`（冻结黑洞引擎时钟）、`?intro=1`（强制播开场）、
`?intro=1&it=<秒>`（冻结开场时钟）。

## 2. 已建成清单（按波次，均已主人验收）

- **场景引擎** `src/lib/scene/`：黑洞全屏 shader（blackhole.ts，机位 preset/缓动、
  presence 状态机、Canvas2D 降级）；行星+星环（PlanetLayer，行星自转/星环 28s 自转、
  鼠标视差近大远小）；四档时间线照明（tokens.ts/timeline.ts，暖白行照→熄灯影院）。
- **三模式**（App.svelte mode state）：陪伴=舰桥（默认）；工程=深舱
  （`src/lib/cabin/DeepCabinLayer.svelte`，底图 `src/assets/cabin/deep-cabin.webp`）；
  专注=临渊（只留黑洞星空，点黑洞进入，Esc/胶囊返回）。右缘三段切换器。
- **开场动画「火之文明史」**：**已封存（2026-08-22，审美未过主人验收）**，默认关闭。
  全部细节、审美债、重启要点见 [intro-animation.md](intro-animation.md)。
- **后端联调**（2026-08-22，本轮）：见下节。

## 3. 后端联调现状（:8090 真实数据在流）

契约即真相：[../02-guides/frontend-data-contract.md](../02-guides/frontend-data-contract.md)（17+1 条路由，字段级带代码行号）。

- **已通**：`/health`、`/v1/models`、`/v1/chat/completions`（非流式+SSE 流式，
  CoT `<think>` 段前端分流到 reasoning 通道）、`/v1/panel/*`（sessions/timeline/
  streams/episodes/graph/audit）、`/v1/apeireth/approval-requests`、`/v1/apeireth/grant`、
  `/v1/apeireth/events`（SSE）、`/v1/tools/list`（本轮新增，34 工具只读投影）。
- **本轮后端改动**（companion_serve.rs，最小侵入）：① CorsLayer（本地开发放开，
  与 apeireth-api/server.rs R27 同款）；② `GET /v1/tools/list`（数据源=发给 LLM 的
  同一真注册表）。Cargo.toml 加 tower-http cors（复用仓库已有版本）。
- **本轮前端改动**：runtime.ts（fetchGraphData 适配真形状；streamChat CoT 增量分流，
  含跨 chunk 边界）、MemoryView.svelte（timestamp=0 隐藏 1970 时间行）。
- **仍是 404/空态（设计内降级，非事故）**：`/v1/apeireth/capabilities`、`/v1/memory/append`、
  `/v1/organs`、V2 mutation 族；图谱页真空（提炼器未产出 fact，等后端"做梦"积累）。

## 4. Tauri 壳（桌面端，`src-tauri/`）

> 状态：**v1 完工**（2026-08-22，wave7）。壳已实机验收：主窗/托盘/单实例/quick 窗全通，
> 后端自拉起逻辑就绪（本机后端已在跑，探测正确跳过）。下方日志含全部环境坑。

**本轮 v1 计划（克制，不扩 scope）**：

1. 壳跑起来：`pnpm tauri dev` 起真实窗口（devUrl `127.0.0.1:1420`，vite.config
   strictPort 已对齐；起服前查 `netstat -ano | grep -E ':(1420|8090)'`）。
2. 单实例：`tauri-plugin-single-instance`——二次启动聚焦已有主窗，不再开新实例。
3. 后端自拉起：壳 setup 时探测 `127.0.0.1:8090`（TCP connect 短超时），没在听就 spawn
   `target/debug/examples/companion_serve.exe`（相对仓库根解析，env 继承父进程；
   `APEIRETH_API_KEY` 等密钥**不入码不打印**，靠环境继承）。spawn 失败/端口仍不通 =
   可接受的 v1 行为——前端健康门控自然显示"未连接"，壳不阻塞 UI。
4. quick 窗最小视图：`?window=quick` 分支——紧凑对话（输入框 + 最近一条回复 +
   连接状态点），无 rail/无 chrome/无场景层，深色透明底配壳的 transparent 窗。

**当前状态**：v1 已验收（截图 `_research_mem/wave7-shots/`）。

**已落码清单**（细节见日志）：

- `Cargo.toml` +`tauri-plugin-single-instance = "2"`。
- `lib.rs`：builder 最前注册单实例插件（回调 unminimize+show+focus 主窗）；setup 挂
  独立线程 `ensure_backend_running`（/health 探测→缺则 spawn，CREATE_NO_WINDOW，
  spawn 后 2s 复探只记日志）；托盘 +`tooltip("Apeireth 伙伴")`。
- `capabilities/default.json` +`core:window:allow-start-dragging`（quick 窗拖动条用）。
- quick 视图：分支点**早已在 `main.ts`**（挂载层 `window=quick` → `features/quick/
  QuickWindowView.svelte`，不是 App.svelte——对 1725 行主文件零侵入）。本轮补齐：
  头部连接状态点（`checkHealthDetailed`，挂载即探 + 15s 轮询，busy 显示"生成中"）、
  `onMount` 挂/卸 `html.companion-bg` 透明钩子、`base.css` 钩子注释同步。
  WebView2 透明**实机正常**（圆角可透视，无白底）。
- 启动器环境坑（`_research_mem/tauri-dev-launch.cmd` 已固化）：本机 cargo 与 npm 全局
  目录**都不在系统 PATH**——tauri CLI 靠 `CARGO` 环境变量定位 cargo；
  `beforeDevCommand pnpm dev` 需要 `C:\Users\31683\AppData\Roaming\npm` 在 PATH；
  `CARGO_NET_OFFLINE=true` 可离线解析（插件 2.4.3 本地缓存命中）。

**施工日志**（最新在上）：

- 2026-08-22 14:15 **v1 完工验收**：`cargo check` 47.6s 全绿 → `pnpm check` 0/0 →
  `pnpm tauri dev` 全量编译 1m08s 起窗。tasklist 确认 `companion-desktop.exe`（首起
  PID 52928；补托盘 tooltip 触发热重载后 PID 51772）。日志打出 `[companion-shell]
  后端 :8090 /health 已在听, 跳过自拉起` ✓。截图亲审：主窗（PrintWindow 直采，标题栏
  "Apeireth 伙伴"、"已连接"、黑洞/行星/星环场景完整）；托盘菜单（打开主窗/快捷窗口/退出
  三项全对）；quick 窗（紧凑视图 + 绿色状态点 + 透明圆角正常）。**单实例实测**：二次
  启动 exe 后进程数仍=1（51772），聚焦已有主窗。spawn 路径未实弹演练（后端本来在跑），
  逻辑经编译+审查，如实记录。

- 2026-08-22 13:40 Rust 侧完工：`Cargo.toml` +`tauri-plugin-single-instance = "2"`（照抄
  现有版本风格）；`lib.rs` builder 最前注册单实例插件（二次启动 unminimize+show+focus
  主窗），setup 挂 `std::thread::spawn(ensure_backend_running)`——探测
  `127.0.0.1:8090/health`（TCP+最小 GET，500/800ms 超时），200 跳过 / 被占非 200 跳过 /
  连不上才 spawn `target/debug/examples/companion_serve.exe`（`env!("CARGO_MANIFEST_DIR")`
  锚定仓库根，cwd=仓库根，CREATE_NO_WINDOW 不弹黑窗，spawn 后 2s 复探只记日志）。
  `capabilities/default.json` +`core:window:allow-start-dragging`（quick 无边框窗拖动）。
  `cargo check` 47.6s 全绿（CARGO_NET_OFFLINE=true，插件 2.4.3 本地缓存命中）。

- 2026-08-22 13:34 骨架建立。盘点结论：端口 8090 在听（PID 45228，不动它）、1420 空闲；
  vite.config.ts 已锁 1420 strictPort；`target/debug/examples/companion_serve.exe` 存在；
  cargo 在 `C:\Users\31683\.cargo\bin\`（不在 bash PATH，用全路径调用）；
  Cargo.toml 已有 autostart/notification（版本风格 `= "2"`，照抄）。

## 5. 接手者必读的坑与纪律（全是血换来的）

1. **planet-xfade 黑箱坑**：`.planet-xfade` 包裹层必须保持 `position: static`——
   定位祖先会把 PlanetLayer 的 screen 混合隔离成黑箱盖死黑洞。IntroLayer 等新层不得破坏。
2. **服务器纪律**：先 `netstat -ano | grep ':5199' | grep LISTENING` 查占用；
   杀服 `taskkill //F //T //PID <pid>` 精确杀进程树，**绝不碰其他 node/esbuild 进程**
   （会误杀 Kimi 预览服务器）。截完图必杀前端 dev 服；后端 :8090 可留。
3. **0 装 PASS**：任何视觉改动必须 Edge 无头截图 + ReadMediaFile 亲审才可交付——
   `"/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe" --headless
   --window-size=1600,900 --screenshot=<abs> --virtual-time-budget=4000 "<url>"`，
   用 `?t0=`/`?it=` 冻结时钟截任意时刻。交付图片链接用**正斜杠**路径（反斜杠客户端报不存在）。
4. **check 0/0 是硬门**：`pnpm check` 0 errors 0 warnings 才算完。
5. **密钥纪律**：LLM key / master token 可读环境/主人本地文件用，**永不打印、永不入库**；
   前端 apiKey/masterToken 只存内存不落盘（runtime.ts:126-178）。
6. **GLSL 陷阱**：`active` 等保留字不能当变量名（编译失败会静默走降级）；
   发光 sprite 衰减必须在 quad 边缘硬归零（否则 additive 下漏方框边）。

## 6. 欠账与下一步（按优先级）

1. ~~**Tauri 桌面壳跑起来**（窗口/托盘/开机自启；后端随壳拉起或自动连接）~~
   **✅ v1 完工（2026-08-22 wave7，见 §4）**：窗口/托盘/单实例/quick 窗/后端自拉起探测
   全验收。残余小账：开机自启插件已挂但 UI 未接开关；spawn 路径未实弹演练过（后端
   一直在跑）；quick 窗暂无全局热键（lib.rs 注释里的 Alt+Space 未实装）。
2. streamChat 的 CoT 流尾 flush（[DONE] 时末尾残片不补发，影响极小）。
3. 开场动画重启打磨（额度充足后，审美债清单在 intro-animation.md）。
4. 播放期键盘 1/2/3 穿透底层场景未堵。
5. 设置页"重看开场"入口；工程模式对话页无 composer（倾向点对话直接送回陪伴模式）。
6. 桌宠（二次元 Ta / Live2D）线：主人同意先放一边，Tauri 验证后端优先。
   —— 壳地基已就绪（§4），可重启评估。

## 7. 资产索引

- 实拍验收图：`previews/`（三模式、时间线、后端联调）、`_research_mem/wave3~6-shots/`
- 开场分镜+概念图：`_research_mem/intro-storyboard/storyboard.md`、`beat-01~09.png`
- 设计系统：`01-DESIGN-SYSTEM.md`；数据契约：`../02-guides/frontend-data-contract.md`
- 施工集群惯例：Agent 工具 coder 施工、指挥亲审验收；断连后先 `git status`+端口盘点再续工
