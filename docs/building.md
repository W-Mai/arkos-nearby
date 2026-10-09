# 从源码构建

## 工具与资产

源码包含运行程序、GUI 和核心查询器；无线模块、supplicant 和 C++ 运行库按已验证的二进制资产打包。下载 [build-assets](https://github.com/W-Mai/arkos-nearby/releases/download/v0.2.0/arkos-nearby-0.2.0-build-assets.tar.gz)，构建准备步骤核对归档摘要和每个文件的大小、摘要、类型与清单。对应上游源码随同一 Release 提供，见 [第三方组件](third-party.md)。

需要 Python 3.11+、Rust 1.92.0、ARM64 musl 标准库、rustfmt、clippy、clang、llvm-ar 和 Ruff。宿主可以是 Linux 或 macOS；交叉构建使用 Rust 自带的 rust-lld。先准备这些工具，再执行下面的构建命令。

## 准备

```sh
python3 scripts/build.py prepare /path/to/arkos-nearby-0.2.0-build-assets.tar.gz
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

## 发布来源

`release/provenance.json` 记录源码文件的 SHA-256，`release/manifest.json` 记录嵌入资产。`SHA256SUMS` 和 `build-info.json` 保存发行文件的校验值与构建记录。

发布宿主上已完成公开源码重建，运行程序 SHA-256 为 `6a69131ab965ba71847728efdf0fbe937631f3f5e892b9cef126b48cc65d5786`，GUI 为 `3911d9e202064991930b44962f375f4e7f0e93eb23d8eb4e45f6a915923adb0b`，均与设备已安装资产相同。
