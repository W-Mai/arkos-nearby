# ArkOS Nearby 0.12.0

- 支持独立启动器与默认首页设置。
- 支持设备重命名、中文拼音输入和好友管理。
- 增加版本、诊断与本地更新包安装入口。
- 修复菜单图标跳位、列表底部裁切和详情滚动未刷新的问题。
- 完整显示存档详情与会话路径，返回时保留选中位置。

![双机联机演示](https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.12.0/docs/assets/nearby-dual-demo.svg)

## 安装

下载 **arkos-nearby-0.12.0-arm64.tar.gz**，解压后将 `arkos-nearby` 文件夹放入游戏卡的 `ports` 目录，在掌机 **Ports → Install Nearby Multiplayer** 中安装。两台掌机安装同一版本，安装完成后重启掌机。

[安装与更新](https://github.com/W-Mai/arkos-nearby/blob/v0.12.0/docs/installation.md) · [游戏兼容性](https://github.com/W-Mai/arkos-nearby/blob/v0.12.0/docs/compatibility.md)

## 源码与校验

`SHA256SUMS` 提供各下载文件的 SHA-256。`build-info.json` 记录应用、界面和构建资产的版本与摘要。Rust 依赖源码、SDL2、无线组件、Nestopia 和 GCC 源码归档随 Release 提供。

[固定内核源码](https://github.com/W-Mai/arkos-nearby/releases/download/v0.10.0/arkos-kernel-c6b78a0.tar.gz) · SHA-256 `a163b8c7d8064c554285fda24cf14a8a5e38947bd8778d69525f5589cafb1dee`
