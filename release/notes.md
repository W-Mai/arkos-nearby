# ArkOS Nearby 0.3.0

联机状态同步与启动性能更新。

## 更新

- **NES / Nestopia**：补全输入状态恢复，清理序列化缓冲区的填充数据，减少重复状态同步。
- **SNES**：降低整状态传输的压缩开销，《超级炸弹人 5》保留已支持的本地核心。
- **街机**：为匹配已验证游戏内容与核心构建的《快打旋风》选择 FBNeo。
- **手柄与同步**：统一两端手柄端口，周期状态校验间隔调整为 600 帧。

![两台 R36S 准备开始游戏](https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.3.0/docs/photos/room-ready.jpg)

## 安装与更新

下载 **`arkos-nearby-0.3.0-arm64.tar.gz`**，解压后将 `arkos-nearby` 文件夹放入游戏卡的 `ports` 目录，在掌机 **Ports → Install Nearby Multiplayer** 中安装。已有版本使用相同方式更新，两台掌机安装同一版本。

适用配置：**ArkOS4Clone 08262026 · Linux ARM64 4.4.189 · RTL8188EU · 640×480 屏幕**。

游戏加载前按住 **X**，或从 **Options → Nearby Multiplayer** 选择游戏。一台创建房间，另一台选中房间加入，双方准备完成后由房主开始游戏。

[安装与更新](https://github.com/W-Mai/arkos-nearby/blob/v0.3.0/docs/installation.md) · [平台与游戏](https://github.com/W-Mai/arkos-nearby/blob/v0.3.0/docs/compatibility.md) · [源码构建](https://github.com/W-Mai/arkos-nearby/blob/v0.3.0/docs/building.md)

## 下载文件

| 文件 | 用途 |
| --- | --- |
| `arkos-nearby-0.3.0-arm64.tar.gz` | 掌机安装包 |
| `SHA256SUMS` | 下载校验值 |
| `build-info.json` | 构建记录 |
| `arkos-nearby-0.3.0-source.tar.gz` | 应用源码 |
| `arkos-nearby-0.3.0-build-assets.tar.gz` | 源码构建所需资产 |
| `nestopia-7dfdc25-footer1-source.tar.gz` | Nestopia 源码、状态恢复补丁和构建脚本 |
| `arkos-nearby-0.3.0-wireless-source.tar.gz` | supplicant 源码、广告桥补丁和配置 |
| `arkos-nearby-0.3.0-rust-sources.tar.gz` | Rust 依赖源码 |
| `arkos-kernel-c6b78a0.tar.gz` | 无线模块对应的 BSP 源码 |
| `gcc-16-16-20260315-source.tar.gz` | C++ 运行库对应的源包、打包补丁和 `.dsc` |
