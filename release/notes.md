# ArkOS Nearby 0.2.0

R36S 双机附近联机。游戏加载前按住 X，或从 Options → Nearby Multiplayer 选择游戏，创建或加入附近房间。首次连接由房主确认，双方准备完成后开始游戏；退出后恢复之前连接的 Wi-Fi。

![双机准备开始游戏](https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.2.0/docs/photos/room-ready.jpg)

## 安装

下载 `arkos-nearby-0.2.0-arm64.tar.gz`，解压后将 `arkos-nearby` 文件夹放入游戏卡的 `ports` 目录，在掌机 Ports 中打开 **Install Nearby Multiplayer**。两台安装同一版本，并准备相同游戏和匹配核心。

适用配置：**ArkOS4Clone 08262026 · Linux ARM64 4.4.189 · RTL8188EU · 640×480 屏幕**。

[安装与更新](https://github.com/W-Mai/arkos-nearby/blob/v0.2.0/docs/installation.md) · [平台与游戏](https://github.com/W-Mai/arkos-nearby/blob/v0.2.0/docs/compatibility.md) · [源码构建](https://github.com/W-Mai/arkos-nearby/blob/v0.2.0/docs/building.md)

## 双机实拍

<table>
  <tr><td align="center"><b>搜索房间</b></td><td align="center"><b>坦克大战</b></td></tr>
  <tr><td><img src="https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.2.0/docs/photos/room-search.jpg" alt="搜索附近房间" width="400" /></td><td><img src="https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.2.0/docs/photos/battle-city.jpg" alt="坦克大战画面" width="400" /></td></tr>
</table>

![俄罗斯方块](https://raw.githubusercontent.com/W-Mai/arkos-nearby/v0.2.0/docs/photos/tetris.jpg)

## 下载文件

| 文件 | 用途 |
| --- | --- |
| `arkos-nearby-0.2.0-arm64.tar.gz` | 掌机安装包 |
| `SHA256SUMS` | 下载校验值 |
| `build-info.json` | 构建记录 |
| `arkos-nearby-0.2.0-build-assets.tar.gz` | 源码构建所需资产 |
| `arkos-nearby-0.2.0-wireless-source.tar.gz` | supplicant 源码、广告桥补丁和配置 |
| `arkos-nearby-0.2.0-rust-sources.tar.gz` | Rust 依赖源码 |
| `arkos-kernel-c6b78a0.tar.gz` | 无线模块对应的 BSP 源码 |
| `gcc-16-16-20260315-source.tar.gz` | C++ 运行库对应的源包、打包补丁和 `.dsc` |
