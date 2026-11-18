# Howff 0.16.0

- 附近房间显示房主昵称、游戏名称和对应的本地截图。房间出现时，探测器旁的文字逐字飞向列表；最后一个房间消失时，文字沿原路收回。
- 两台掌机上的房主和访客坐在相同的位置。房主可以看到加入请求和开局按键，访客可以看到确认进度；入座、离开和准备就绪都有对应的动作。
- 收藏卡带随翻页滑动，长按 X 时直接在所选游戏上显示联机进度。游戏库为选中游戏留出整行，长标题滚动一次。
- 提供浅色、暖色和深色配色。主页显示时间、电量和无线状态，弹窗展开、收起和中途返回都有连贯的过渡。

![房主和访客的双机房间视角](https://raw.githubusercontent.com/W-Mai/howff/v0.16.0/docs/assets/room-pair.webp)

## 安装与更新

下载本页的 **howff-0.16.0-arm64.tar.gz**。首次安装时，解压并将 `howff` 文件夹复制到游戏卡，在掌机文件管理器中运行 **Install Howff.sh**。

已安装 Howff 时，将安装包归档放到游戏卡，结束游戏与房间，在 **START → 应用管理 → 安装更新** 中选择归档并确认。安装后在应用管理中查看版本，两台掌机使用同一版本联机。

[安装与更新](https://github.com/W-Mai/howff/blob/v0.16.0/docs/installation.md) · [使用说明](https://github.com/W-Mai/howff/blob/v0.16.0/docs/usage.md) · [游戏兼容性](https://github.com/W-Mai/howff/blob/v0.16.0/docs/compatibility.md)

## 源码与校验

`SHA256SUMS` 提供下载文件的 SHA-256，`build-info.json` 记录应用、界面和构建资产的版本与摘要。源码归档、Rust 依赖源码及组件源码随 Release 提供，许可证与来源见源码中的 `docs/third-party.md`。
