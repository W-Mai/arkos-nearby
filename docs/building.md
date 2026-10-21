# 从源码构建

## 工具与资产

源码包含运行程序、GUI、核心查询器、联机压缩辅助库和 Nestopia 状态恢复补丁。构建资产与源码版本配套，归档文件名和 SHA-256 见 [assets.json](../release/assets.json)，嵌入文件清单见 [manifest.json](../release/manifest.json)。发行标签对应的构建资产和上游源码可从 [GitHub Releases](https://github.com/W-Mai/arkos-nearby/releases) 获取，组件来源见 [第三方组件](third-party.md)。

需要 Python 3.11+、Rust 1.92.0、ARM64 musl 标准库、rustfmt、clippy、clang、llvm-ar 和 Ruff。宿主可以是 Linux 或 macOS；交叉构建使用 Rust 自带的 rust-lld。先准备这些工具，再执行下面的构建命令。

## 准备

```sh
python3 scripts/build.py prepare /path/to/arkos-nearby-0.9.1-build-assets.tar.gz
cargo +1.92.0 fetch --locked --manifest-path native/nearby/Cargo.toml --target aarch64-unknown-linux-musl
cargo +1.92.0 fetch --locked --manifest-path gui/nearby/Cargo.toml --target aarch64-unknown-linux-musl
```

`fetch` 是显式联网准备。之后构建和检查使用 `--locked --offline`。配置已有的 clang 与 llvm-ar：

```sh
CC_aarch64_unknown_linux_musl=/path/to/clang AR_aarch64_unknown_linux_musl=/path/to/llvm-ar python3 scripts/build.py build
```

输出为 `dist/arkos-nearby` 和 `dist/SHA256SUMS`。所有 Cargo 编译目录位于 `/tmp/arkos-rust-build/public/`，直接 Cargo 命令使用 `/tmp/arkos-rust-build/public-cargo/`。GUI 固定 mirui 和 mirx 0.51.0，页面使用 `#[compose]`、`ui!` 与 Flex 布局，文字按可用空间换行或省略。构建记录和安装资产校验读取 Cargo.toml 与 Cargo.lock 中的实际版本。

GUI 通过 `scripts/build_nearby_gui.py` 构建和检查，直接使用 Cargo.lock 锁定的 registry 包。构建记录包含 mirui、mirui-macros、mirx 的 crate 校验和、应用源码摘要和实际锁文件摘要。页面由 PageView 挂载和释放，焦点框通过 FollowBounds 与共享动画调度跟随目标。灰度位图字体的采样外缘使用 PaintInflate 计入重绘覆盖。源码、Clippy、测试和 ARM64 构建使用同一份未修改的框架包。

`gui/nearby/src/model.rs` 定义显示协议；`render.rs` 负责安装与 Scene 入口。`render/state.rs` 发布显示状态，`copy.rs` 提供状态文案，`style.rs` 和 `font.rs` 管理尺寸、颜色与字体，`widgets.rs` 组合公共控件。`chrome.rs`、`choices.rs`、`lists.rs`、`sessions.rs` 和 `pages.rs` 构成页面，`composition.rs` 挂载 PageView，`selection.rs` 管理页面内焦点。网络、游戏核对和会话清理仍属于 native runtime。

Options 和主页 FN + X 使用直接入口，游戏启动前的 X 入口携带当前游戏。`MenuSession` 连接附近房间、模式选择、游戏列表与房间流程；`menu/display.rs` 持有 GUI 进程、手柄映射和消息通道。后台回复按屏幕会话隔离，输入保留接收时间检查。`home.rs` 的快捷键监听随前台 EmulationStation 进程运行，进入工具时移交显示，退出后恢复前台。

`native/home_input/` 在 EmulationStation 的 SDL 事件入口消费 FN + X 的 X 按下及松开，保留独立 X 和其他按键。启动器仅为该前端加载 `home-input.so`，子程序继承其余环境。使用已有 ARM64 GNU 编译器和 SDL2 头文件执行 `python3 scripts/build_home_input.py build --compiler /path/to/aarch64-linux-gnu-clang --sdl-include /path/to/SDL2/include`；构建产物和来源校验记录一起进入安装包。

`catalog/metadata.rs` 读取本机游戏列表名称、截图和数量；`catalog/matching.rs` 在客户端取得房间描述后匹配本地内容。候选项以会话内标识选择，准备完成仍需核对模拟器、核心和内容身份。`render/artwork.rs` 异步读取选中游戏的截图，通过资源管理器和 Image 控件显示；过期选择的读取结果会被丢弃，图片保持原始比例。

先执行 `python3 scripts/build_nearby_gui.py build` 生成当前源码对应的 GUI 和来源记录，再执行质量检查。`release/manifest.json` 和 `release/assets.json` 保留已发行安装资产的身份；构建时将当前 GUI、字体许可证及来源记录放入新的本地 bundle。

准备发行归档时，先完成构建和校验，再执行 `python3 scripts/package.py --binary dist/arkos-nearby --expected-sha256 VERIFIED_SHA256`。将 `VERIFIED_SHA256` 替换为构建结果的 SHA-256。程序核对摘要与 ARM64 ELF 类型，打包安装脚本、使用说明和许可证。

## 质量检查

```sh
CC_aarch64_unknown_linux_musl=/path/to/clang AR_aarch64_unknown_linux_musl=/path/to/llvm-ar sh scripts/quality.sh
```

检查 Python 格式和语法、源文件摘要、Rust 格式与警告、GUI 和运行程序单元测试、ARM64 交叉编译及核心查询器 C 源码。可以用 `RUFF=/path/to/ruff` 指定 Ruff 路径。

## 界面截图

界面使用 Fusion Pixel 简体中文像素字体，正文为 12px，标题为 24px。两个字库覆盖 `gui/nearby/assets/nearby-menu-charset.txt` 中的 3856 个字符。字体来源、转换输入和输出摘要见 `font-source.json`，字体及其衍生来源许可证合并保存在 `OFL.txt`。

重新生成字库时，将来源记录中的 WOFF 解码为 OpenType 文件，使用 ICU 0.11.1 执行 `icu bake-font nearby-pixel.otf --charset-file gui/nearby/assets/nearby-menu-charset.txt --size 12 --bit-depth 1 --format gray -O /tmp/nearby-font`；将字号改为 24 生成标题字库。将产物分别命名为 `nearby-menu-12.mirx` 和 `nearby-title-24.mirx`，并更新来源记录中的摘要与大小。

发行版使用 SDL2/GLES 界面；静态链接构建命令及显示检查见 [GPU 界面构建](gpu-rendering.md)。`scripts/build.py build` 生成 framebuffer 安装程序。

```sh
python3 scripts/build_nearby_gui.py snapshot --view docs/screenshots/views/room-choice.json --output /tmp/arkos-room-choice.png
```

截图使用相同的场景和字体渲染代码，输入仅包含显示状态。`docs/screenshots/views/` 保存 README 截图所用的示例状态。

## 双机演示动画

`dual_demo` 使用 mirui 渲染掌机外壳、镜头变换和两个独立的菜单场景。分镜包括创建房间、搜索加入、房主确认、双机就绪、仅组网和主题切换。菜单页面复用应用的布局和动效；演示节奏经过编排。

安装 FFmpeg 和 libwebp 的 img2webp 工具后，执行以下命令生成 840×585、25 fps、20 秒循环 WebP：

```sh
python3 gui/nearby/examples/dual_demo/export.py --output /tmp/nearby-dual-demo.webp
```

Cargo 产物沿用 GUI 的外部缓存目录，1120×780 PNG 帧默认保存到 `/tmp/arkos-nearby-demo-frames`。`--frames` 指定帧目录，`--width` 设置成品宽度，`--quality` 设置编码质量，默认 75；`--lossless` 使用无损编码，`--reuse-frames` 复用已有帧进行编码。`gui/nearby/examples/dual_demo/story.rs` 定义分镜与镜头，`scene.rs` 定义宣传画面，`gui/nearby/src/dual_demo.rs` 负责逐帧渲染和导出。

生成适合分享的 H.264 MP4，默认保留 1120×780 分辨率、25 fps 和完整 20 秒内容：

```sh
python3 gui/nearby/examples/dual_demo/export.py --output /tmp/nearby-dual-demo.mp4 --reuse-frames
```

`--crf` 控制视频压缩强度，默认 22；较小的数值保留更多细节。MP4 使用 YUV 4:2:0 和 faststart，便于浏览器及移动端播放。

导出自包含的动画 SVG：

```sh
python3 gui/nearby/examples/dual_demo/svg.py --output /tmp/nearby-dual-demo.svg
```

导出器通过 mirui SceneRenderer 捕获两个菜单与宣传场景，保存绘制顺序和裁剪，将像素字形转为共享路径，并生成 CSS 动画时间轴。相邻且互不重叠的静态矩形合并为路径，裁剪范围完全覆盖的矩形省去裁剪层，暂未出现的图形按时间轴退出绘制。中间场景和 PNG 帧默认写入 `/tmp/arkos-nearby-svg`；`--frames` 指定目录，`--reuse-scenes` 复用其中的 `scenes.json`。SVG 的播放开销取决于浏览器对矢量节点和动画的处理能力。

## 核心查询器

发布资产带有 32/64 位核心查询器。重新构建需要 `native/core_inspect/runtime-pins.json` 中匹配的 ABI 库和 clang，执行 `python3 scripts/build_core_inspect.py build --libraries /path/to/pinned-libraries --output build/assets`。查询器用于读取核心元数据。

## 联机压缩与自有核心

`native/netplay_compression/` 保存已验证 ARM64 RetroArch 的整状态压缩辅助库源码及 ABI 记录，`scripts/build_netplay_compression.py` 使用现有 glibc sysroot、固定 ABI 库和 clang 离线构建。`native/owned_cores/` 保存 Nestopia 状态恢复补丁、许可证、源码及构建输入摘要；完整重建命令见 [自有核心构建说明](../native/owned_cores/README.md)。两类资产都经过二进制摘要和来源检查，再交给安装包管理。

## 发布来源

`release/provenance.json` 记录源码文件的 SHA-256，`release/manifest.json` 记录嵌入资产。`SHA256SUMS` 和 `build-info.json` 保存发行文件的校验值与构建记录。

发行程序摘要记录在 [provenance.json](../release/provenance.json) 的 `release_build` 字段，实机验证基线记录在 `device_validation` 字段。GUI 与附属资产摘要记录在 [manifest.json](../release/manifest.json)。

## 街机载入检查器

`native/core_probe/` 保存 ARM64 街机载入检查器及 libretro ABI 声明。`python3 scripts/build_core_probe.py check` 检查 C 源码；构建命令、固定运行库与许可证见 [检查器说明](../native/core_probe/README.md)。
