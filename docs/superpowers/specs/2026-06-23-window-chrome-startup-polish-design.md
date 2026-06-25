# macOS 窗口控制与启动首帧治理设计

> 2026-06-24 修订：macOS 不再使用 Iris 右侧自绘交通灯，也不再运行期动态切换 `decorations` / `titleBarStyle`。最终方案为 macOS 原生 decorated overlay chrome：系统左侧红黄绿是唯一窗口控件 owner，Iris 顶栏通过 88px traffic-light spacer 避让，并使用配置期 `trafficLightPosition` 与顶栏中线对齐。

## 背景

Iris 采用 Tauri 2 + React 自绘桌面 chrome。现有实现存在四个体验问题：macOS 绿色按钮只是最大化而非原生全屏，右侧三色按钮顺序与目标设计不一致，Windows 冷启动可能暴露旧标题栏 / 扭曲首帧，Knowledge Orbit 启动动画展示时间过短。

## 设计决策

- macOS 使用系统左侧原生红黄绿，Iris 不渲染自绘窗口控件。
- macOS 原生红黄绿位置由 Tauri 配置期 `trafficLightPosition: { x: 14, y: 16 }` 固定，禁止前端运行期接管或动态重排。
- macOS 系统绿色按钮负责进入 / 退出原生 fullscreen Space，不再由前端绿色按钮承担。
- macOS 与 Windows 标题栏双击都负责最大化 / 还原，不与 fullscreen 语义混用。
- Windows 控件保持标准最小化 / 最大化 / 关闭顺序。
- 窗口动作拆为两个前端内部 helper：native fullscreen 与 window maximize，避免继续把不同平台行为塞进一个 `toggleMaximize` 路径。
- 主窗口仍以 `visible:false` 启动；React 启动层首帧绘制后才调用 `show_main_window_when_ready`。
- `index.html` 必须提供 critical preboot splash，避免 React bundle 加载前暴露空白 / 黑色 WebView。
- Tauri capability 不允许运行期动态 `set_decorations` / `set_title_bar_style`；macOS chrome 形态由配置期 `decorations:true` + `titleBarStyle: Overlay` + `trafficLightPosition` 决定。
- 启动层最短展示时间固定为 1600ms，淡出约 220ms；reduced motion 下禁用轨道旋转和涟漪，但不跳过最短展示。

## 实现边界

- 不新增依赖。
- 不改 Tauri IPC wire shape。
- 不新增独立 splash window。
- 不保留 macOS 自绘交通灯或动态 chrome 兼容路径。
- 不改变笔记数据流、编辑器 schema、数据库或 AI 会话状态。

## 验收标准

- macOS 窗口态：左侧系统原生红黄绿正常显示并在 Iris 顶栏内垂直居中；Iris 标识和 tab 从 88px spacer 后开始。
- macOS 标题栏双击：最大化 / 还原。
- macOS 全屏退出后：标题栏高度、品牌轨和系统窗口控件不漂移；不出现 Iris 自绘灯叠层。
- Windows：冷启动不暴露旧黑色标题栏或扭曲模块；窗口控件仍为最小化 / 最大化 / 关闭。
- 启动层：ready 很快时仍至少展示 1600ms；ready 很慢时等待真实 ready；主窗口 reveal 不早于启动层首帧绘制。

## 测试策略

- `WindowControls` 测试覆盖 macOS 不渲染自绘控件和 Windows 标准顺序。
- `window-actions` 测试覆盖 fullscreen 与 maximize helper 的语义分离。
- `window-drag` 测试覆盖标题栏双击最大化以及交互控件排除。
- `StartupSplash` 测试覆盖绘制后 reveal、1600ms 最短展示、reduced motion 标记。
- `runtime-contracts` 继续覆盖主窗口初始隐藏、Rust setup 不主动 show。
- `runtime-contracts` 覆盖 HTML preboot splash 与 fullscreen window API capability。
