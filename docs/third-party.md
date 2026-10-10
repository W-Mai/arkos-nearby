# 第三方组件与源码

应用自有源码采用 [MIT](../LICENSE)。安装包中的独立组件保留其原始许可证，文本位于 [licenses](licenses/)，具体资产摘要和来源记录位于 [release-sources.json](../release/release-sources.json) 与 [third-party-sources.json](../release/third-party-sources.json)。

| 组件 | 许可证 | 源码与发布文件 |
| --- | --- | --- |
| mirui / mirx 0.47.0 与 Rust 依赖 | 各 crate 声明的 MIT、Apache、BSD、ISC 等许可证 | [锁定依赖记录](../release/rust-dependencies.json)；Release 的 `arkos-nearby-0.3.1-rust-sources.tar.gz` 保存构建使用的原始 crate 归档 |
| Noto Sans CJK SC 字体 | SIL Open Font License 1.1 | [字体许可证](../gui/nearby/assets/OFL.txt) 与 [来源记录](../gui/nearby/assets/font-source.json) |
| RTL8188EU 内核模块 | GPL-2.0 | [许可证](licenses/driver-COPYING.txt)；Release 的 `arkos-kernel-c6b78a0.tar.gz` 提供完整固定 BSP 源码、内核配置和构建文件 |
| wpa_supplicant 2.10 | BSD | [许可证](licenses/wpa-COPYING.txt)；Release 的 `arkos-nearby-0.3.1-wireless-source.tar.gz` 提供原始归档、广告桥、补丁脚本和配置 |
| 私有 ARM64 libstdc++ / libgcc | GPL 与 GCC Runtime Library Exception | [许可证和例外](licenses/gcc-runtime-COPYING.txt) 与 [原始源包记录](../release/source.json)；Release 提供 GCC 原始归档、Debian 打包补丁和 `.dsc` |
| ring 0.17.14 | ISC / MIT / Apache 等原始组件许可证 | [许可证](licenses/ring-LICENSE.txt) 与 Rust 源归档 |
| Netplay 压缩辅助库 | MIT | [源码与 ABI 来源](../native/netplay_compression/abi-source.json)、[许可证](licenses/netplay-compression-COPYING.txt) |
| 街机载入检查器的 libretro ABI 声明 | MIT | [源码与构建说明](../native/core_probe/README.md)、[许可证](licenses/core-probe-ABI-LICENSE.txt) |
| Nestopia 1.53.2 状态恢复修正版 | GPL-2.0 | [源码与构建输入记录](../native/owned_cores/nestopia/source.json)、[补丁](../native/owned_cores/nestopia/state-footer.patch)、[许可证](../native/owned_cores/nestopia/COPYING)；`nestopia-7dfdc25-footer1-source.tar.gz` 包含固定上游源码、补丁及构建脚本 |

无线模块取自固定的 ArkOS4Clone 提交，驱动版本为 `v5.13.3-17-gb1925f81a.20210615`。对应 BSP 源码提供完整内核和 `clone_defconfig` 构建流程。supplicant 使用 Realtek 广告桥发布房间信息。

GCC 源包为 `gcc-16`、版本 `16-20260315-1ubuntu1~18~ppa3`。`.dsc` 的 SHA-256 校验值用于核对原始归档和打包补丁。RetroArch、游戏、BIOS 和其他模拟器核心使用掌机已有的文件；Nestopia 修正版安装在工具自有目录，对应源码由记录中的精确上游归档、状态补丁和构建脚本组成。

下载源归档见 [v0.3.1 Release](https://github.com/W-Mai/arkos-nearby/releases/tag/v0.3.1)。发布文件的整体校验值见同页的 `SHA256SUMS`。
