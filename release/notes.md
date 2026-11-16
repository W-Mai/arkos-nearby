# Howff 0.14.0

- 优化游戏库切换、列表浏览和附近房间列表的响应。
- 修复更新时启动入口和平台组件的保留与安装。
- 安装包包含 ArkOS 平台插件，统一游戏目录、核心来源和系统工具信息。
- 按图片来源加载游戏封面、截图和存档预览。
- 修复应用与系统工具往返的前台占用记录。
- 修正内置 SDL2 的 GPU 显示与音效输出构建配置。

![双机联机演示](https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.14.0/docs/assets/nearby-dual-demo.svg)

## 安装与更新

下载本页的 **howff-0.14.0-arm64.tar.gz**。首次安装时，解压并将 `howff` 文件夹复制到游戏卡，在掌机文件管理器中运行 **Install Howff.sh**。

已安装 Howff 时，将安装包归档放到游戏卡，结束游戏与房间，在 **START → 应用管理 → 安装更新** 中选择归档并确认。安装后在应用管理中查看版本，两台掌机使用同一版本联机。

[安装与更新](https://github.com/W-Mai/arkos-nearby/blob/v0.14.0/docs/installation.md) · [使用说明](https://github.com/W-Mai/arkos-nearby/blob/v0.14.0/docs/usage.md) · [游戏兼容性](https://github.com/W-Mai/arkos-nearby/blob/v0.14.0/docs/compatibility.md)

## 源码与校验

`SHA256SUMS` 提供下载文件的 SHA-256，`build-info.json` 记录应用、界面和构建资产的版本与摘要。源码归档、Rust 依赖源码及组件源码随 Release 提供，许可证与来源见源码中的 `docs/third-party.md`。
