# 从源码构建

## 工具与资产

源码包含运行程序、GUI、核心查询器、联机压缩辅助库和 Nestopia 状态恢复补丁。构建资产与源码版本配套，归档文件名和 SHA-256 见 [assets.json](../release/assets.json)，嵌入文件清单见 [manifest.json](../release/manifest.json)。发行标签对应的构建资产和上游源码可从 [GitHub Releases](https://github.com/W-Mai/arkos-nearby/releases) 获取，组件来源见 [第三方组件](third-party.md)。

需要 Python 3.11+、Rust 1.92.0、ARM64 musl 标准库、rustfmt、clippy、clang、llvm-ar 和 Ruff。宿主可以是 Linux 或 macOS；交叉构建使用 Rust 自带的 rust-lld。先准备这些工具，再执行下面的构建命令。

## 准备

```sh
python3 scripts/build.py prepare /path/to/arkos-nearby-0.3.0-build-assets.tar.gz
cargo +1.92.0 fetch --locked --manifest-path native/nearby/Cargo.toml --target aarch64-unknown-linux-musl
cargo +1.92.0 fetch --locked --manifest-path gui/nearby/Cargo.toml --target aarch64-unknown-linux-musl
```

`fetch` 是显式联网准备。之后构建和检查使用 `--locked --offline`。配置已有的 clang 与 llvm-ar：

```sh
CC_aarch64_unknown_linux_musl=/path/to/clang AR_aarch64_unknown_linux_musl=/path/to/llvm-ar python3 scripts/build.py build
```

输出为 `dist/arkos-nearby` 和 `dist/SHA256SUMS`。所有 Cargo 编译目录位于 `/tmp/arkos-rust-build/public/`，直接 Cargo 命令使用 `/tmp/arkos-rust-build/public-cargo/`。GUI 固定 mirui 0.47.0。

准备发行归档时，先完成构建和校验，再执行 `python3 scripts/package.py --binary dist/arkos-nearby --expected-sha256 VERIFIED_SHA256`。将 `VERIFIED_SHA256` 替换为构建结果的 SHA-256。程序核对摘要与 ARM64 ELF 类型，打包安装脚本、使用说明和许可证。

## 质量检查

```sh
CC_aarch64_unknown_linux_musl=/path/to/clang AR_aarch64_unknown_linux_musl=/path/to/llvm-ar sh scripts/quality.sh
```

检查 Python 格式和语法、源文件摘要、Rust 格式与警告、GUI 和运行程序单元测试、ARM64 交叉编译及核心查询器 C 源码。可以用 `RUFF=/path/to/ruff` 指定 Ruff 路径。

## 界面截图

```sh
python3 scripts/build_nearby_gui.py snapshot --view docs/screenshots/views/room-choice.json --output /tmp/arkos-room-choice.png
```

截图使用相同的场景和字体渲染代码，输入仅包含显示状态。`docs/screenshots/views/` 保存 README 截图所用的示例状态。

## 核心查询器

发布资产带有 32/64 位核心查询器。重新构建需要 `native/core_inspect/runtime-pins.json` 中匹配的 ABI 库和 clang，执行 `python3 scripts/build_core_inspect.py build --libraries /path/to/pinned-libraries --output build/assets`。查询器用于读取核心元数据。

## 联机压缩与自有核心

`native/netplay_compression/` 保存已验证 ARM64 RetroArch 的整状态压缩辅助库源码及 ABI 记录，`scripts/build_netplay_compression.py` 使用现有 glibc sysroot、固定 ABI 库和 clang 离线构建。`native/owned_cores/` 保存 Nestopia 状态恢复补丁、许可证、源码及构建输入摘要；完整重建命令见 [自有核心构建说明](../native/owned_cores/README.md)。两类资产都经过二进制摘要和来源检查，再交给安装包管理。

## 发布来源

`release/provenance.json` 记录源码文件的 SHA-256，`release/manifest.json` 记录嵌入资产。`SHA256SUMS` 和 `build-info.json` 保存发行文件的校验值与构建记录。

发行程序摘要记录在 [provenance.json](../release/provenance.json) 的 `release_build` 字段，实机验证基线记录在 `device_tested_build` 字段；`source_adjustments` 记录发行版本号的调整。GUI 与附属资产摘要记录在 [manifest.json](../release/manifest.json)。
