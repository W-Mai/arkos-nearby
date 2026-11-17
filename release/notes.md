# Howff 0.15.0

- 统一 Howff 标识与页头，联机面板显示当前游戏或所选房间。
- Select、FN+X 和 Options 始终先显示附近房间，无需普通 Wi-Fi 的 IP 地址。
- 调整紧凑屏菜单、中文输入、好友详情与存档摘要，修复文字遮挡。
- 完善像素字体的中文、日文和标点显示。
- 长按 X 时保持收藏布局和底部操作提示稳定。
- 更新 Logo、字标、界面截图与双机演示。

![双机联机演示](https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.15.0/docs/assets/nearby-dual-demo.svg)

## 安装与更新

下载本页的 **howff-0.15.0-arm64.tar.gz**。首次安装时，解压并将 `howff` 文件夹复制到游戏卡，在掌机文件管理器中运行 **Install Howff.sh**。

已安装 Howff 时，将安装包归档放到游戏卡，结束游戏与房间，在 **START → 应用管理 → 安装更新** 中选择归档并确认。安装后在应用管理中查看版本，两台掌机使用同一版本联机。

[安装与更新](https://github.com/W-Mai/arkos-nearby/blob/v0.15.0/docs/installation.md) · [使用说明](https://github.com/W-Mai/arkos-nearby/blob/v0.15.0/docs/usage.md) · [游戏兼容性](https://github.com/W-Mai/arkos-nearby/blob/v0.15.0/docs/compatibility.md)

## 源码与校验

`SHA256SUMS` 提供下载文件的 SHA-256，`build-info.json` 记录应用、界面和构建资产的版本与摘要。源码归档、Rust 依赖源码及组件源码随 Release 提供，许可证与来源见源码中的 `docs/third-party.md`。
