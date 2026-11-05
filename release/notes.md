# ArkOS Nearby 0.11.0

- 在启动器里连接 Wi-Fi：扫描网络、输入密码、查看连接进度，取消时恢复原网络。
- 编辑游戏、核心和前端设置，查看继承值、生效预览与保存位置。
- 管理即时存档和游戏进度：按槽位载入、创建备份、确认恢复，返回时保留原游戏和选中位置。
- 查看游戏核心来源、BIOS 文件及实际存档和配置路径。
- 使用像素滑块调节音量和亮度，查看实际百分比。
- 查看分区容量、可用空间和挂载信息，通过电源菜单确认关机或重启。
- 收藏架支持按可用空间排列多行、跨页导航和动态页码，菜单返回后恢复焦点与小人位置。
- 加快存档列表加载，修复返回存档菜单后的内容重复，以及 Wi-Fi 重扫失败时房间列表消失的问题。

![双机联机演示](https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.11.0/docs/assets/nearby-dual-demo.svg)

## 安装与更新

下载 **arkos-nearby-0.11.0-arm64.tar.gz**，解压后将 `arkos-nearby` 文件夹放入游戏卡的 `ports` 目录，在掌机 **Ports → Install Nearby Multiplayer** 中安装。两台掌机安装同一版本，安装完成后重启掌机。

适用配置：**ArkOS4Clone 08262026 · Linux ARM64 4.4.189 · RTL8188EU · 640×480 屏幕**。

[安装与更新](https://github.com/W-Mai/arkos-nearby/blob/v0.11.0/docs/installation.md) · [游戏兼容性](https://github.com/W-Mai/arkos-nearby/blob/v0.11.0/docs/compatibility.md)

组件源码：[固定 BSP 内核源码](https://github.com/W-Mai/arkos-nearby/releases/download/v0.10.0/arkos-kernel-c6b78a0.tar.gz)。该归档的校验值列在本版 `SHA256SUMS` 中。
