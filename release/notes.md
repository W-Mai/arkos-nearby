# Howff 0.18.0

- 反复按 Select 打开联机面板时，入场更流畅。仅组网后再打开，面板会带着当前连接状态展开。
- 仅组网的无线图标在两台掌机上居中，等待确认、连接中和组网完成各有对应的动画。
- 游戏库和房间的选框会滑到下一个目标，选中的按钮和列表项会抬起，游戏平台预览随选择逐步拼装。
- 收藏翻页使用弹簧动画。选中卡带时外壳抬起，切角和金手指随之露出，长按 X 的联机提示贴在选框上。
- 剩余电量直接显示在电池图标内。

## 安装与更新

下载本页的 **howff-0.18.0-arm64.tar.gz**。首次安装时，解压并将 `howff` 文件夹复制到游戏卡，在掌机文件管理器中运行 **Install Howff.sh**。

已安装 Howff 时，将安装包归档放到游戏卡，结束游戏与房间，在 **START → 应用管理 → 安装更新** 中选择归档并确认。安装后在应用管理中查看版本，两台掌机使用同一版本联机。

[安装与更新](https://github.com/W-Mai/howff/blob/v0.18.0/docs/installation.md) · [使用说明](https://github.com/W-Mai/howff/blob/v0.18.0/docs/usage.md) · [游戏兼容性](https://github.com/W-Mai/howff/blob/v0.18.0/docs/compatibility.md)

## 源码与校验

`SHA256SUMS` 提供下载文件的 SHA-256，`build-info.json` 记录应用、界面和构建资产的版本与摘要。应用源码随安装包发布。Rust 依赖与第三方组件的源码下载见下方链接，许可证与来源见 `docs/third-party.md`。
