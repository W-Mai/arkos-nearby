# Howff 0.13.0

- 使用 Mirui 0.56.0，保留 GPU 渲染、离屏纹理和界面动效。
- 优化收藏、游戏库和联机面板的列表复用与页面切换。
- 保留独立启动器、单机存档、联机存档隔离和退出恢复网络。

- 支持独立启动器与默认首页设置。
- 支持设备重命名、中文拼音输入和好友管理。
- 提供版本信息、设备诊断与本地安装包更新，可安装所选版本并继续进入应用。
- 核心来源标明「工具优化」或「设备原有」，加入房间使用房主确定的核心。
- 更新和重复安装保留收藏、最近游玩、偏好、身份与好友，游戏列表缺失或损坏的索引自动重建。
- 修复游戏列表加载完成后，选中项跳到其他游戏的问题。
- 修复菜单图标跳位、列表底部裁切和详情滚动未刷新的问题。
- 完整显示存档详情与会话路径，返回时保留选中位置。

![双机联机演示](https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.13.0/docs/assets/nearby-dual-demo.svg)

## 安装与更新

下载本页的 **howff-0.13.0-arm64.tar.gz**。首次安装时，解压并将 `howff` 文件夹复制到游戏卡，在掌机文件管理器中运行 **Install Howff.sh**。

已安装 Howff 时，将安装包归档放到游戏卡，结束游戏与房间，在 **START → 应用管理 → 安装更新** 中选择归档并确认。安装后在应用管理中查看版本，两台掌机使用同一版本联机。

[安装与更新](https://github.com/W-Mai/arkos-nearby/blob/v0.13.0/docs/installation.md) · [使用说明](https://github.com/W-Mai/arkos-nearby/blob/v0.13.0/docs/usage.md) · [游戏兼容性](https://github.com/W-Mai/arkos-nearby/blob/v0.13.0/docs/compatibility.md)

## 源码与校验

`SHA256SUMS` 提供下载文件的 SHA-256，`build-info.json` 记录应用、界面和构建资产的版本与摘要。源码归档、Rust 依赖源码及组件源码随 Release 提供，许可证与来源见源码中的 `docs/third-party.md`。
