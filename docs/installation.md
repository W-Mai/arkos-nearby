# 安装、更新与卸载

## 下载与校验

两台掌机安装同一个版本。下载 [arkos-nearby-0.9.1-arm64.tar.gz](https://github.com/W-Mai/arkos-nearby/releases/download/v0.9.1/arkos-nearby-0.9.1-arm64.tar.gz) 和 [SHA256SUMS](https://github.com/W-Mai/arkos-nearby/releases/download/v0.9.1/SHA256SUMS)。在下载目录执行 `sha256sum --ignore-missing -c SHA256SUMS`；macOS 可使用 `shasum -a 256 -c SHA256SUMS` 并只核对实际下载的文件。

解压后包含 `arkos-nearby`、`Install Nearby Multiplayer.sh`、安装说明、许可证和可执行文件校验值。

## 从游戏卡安装

将解压出的 `arkos-nearby` 文件夹复制到游戏卡的 `ports` 目录。在掌机的 Ports 中打开 `Install Nearby Multiplayer.sh`。程序与脚本必须放在同一目录。安装器检查实际内核、无线芯片、屏幕、系统工具和启动脚本结构，通过后安装应用并添加手动入口。

安装完成后重启掌机。通过 Options → Nearby Multiplayer 或主页 FN + X 打开工具，已连接 Wi-Fi 时会显示附近房间。向下移动或按 R 展开「仅组网／选择游戏」，向上移动或按 L 返回房间列表。单独按 X 保留主页的随机选择功能。游戏开始加载前按住 X，可直接进入当前游戏的创建／加入房间界面。

在选择模式或创建／加入房间的入口页按 Y 切换松林、落日和夜营主题。配色会自动保存。

## 游戏库

选择游戏平台可查看游戏数量、主机图案与游戏截图。游戏列表中按 X 收藏当前游戏；平台列表顶部的「收藏夹」以横向卡带展示收藏，左右移动选择，按 Y 切换名称或最近使用排序，按 X 移除收藏。

在平台列表按 Y 打开搜索，使用方向键和 A 输入游戏名、文件名或中文名的拼音／首字母。按 Y 退格，按 START 切换键盘与结果列表，按 B 收起键盘或返回平台列表。在结果列表中按 X 收藏，按 A 选择游戏并进入创建／加入房间。

收藏与最近使用记录保存在掌机上。再次打开游戏库时会恢复收藏和排序方式。

## 启动器

Options → Nearby Launcher 直接打开个人收藏。横向卡带按可用空间排成一排或两排，方向键移动选择，在同排边缘继续按左右可翻页。选中收藏末尾的游戏库图标，按 A 展开游戏库，收藏架缩小并保留在上方；B 逐级返回，恢复原来的选中卡片。SELECT 打开附近联机面板。

选中游戏后，短按 X 并松开启动单机，长按 X 打开当前游戏的联机面板；A 打开游戏操作，可将游戏移出收藏。在游戏库的游戏列表和搜索结果中按 Y 收藏，收藏页按 Y 排序，平台列表按 Y 搜索。空平台与空文件夹会自动隐藏。游戏退出后返回刚才的列表和选中位置。

单机沿用系统的模拟器设置、游戏存档与即时存档位置。START 打开主题、音效、直连网络管理和退出菜单；选择「返回原系统」回到系统主页。

## 仅组网

选择「仅组网」后，一台创建房间，另一台选择该房间加入。连接完成后显示双方地址，按 A「保持连接，返回系统」可继续从 Ports 打开局域网游戏，并在游戏内创建或加入房间。

再次打开 Nearby Multiplayer 可查看连接，按 B「断开并恢复 Wi-Fi」结束组网。连接意外中断超过 15 秒时，程序恢复原 Wi-Fi。

## 通过 SSH 安装

先在掌机 Options 中启用远程服务，查看当前 IP。将程序复制到可写目录，在设备上使用 sudo 执行：

```sh
scp arkos-nearby ark@HANDHELD_IP:/home/ark/arkos-nearby-installer
ssh ark@HANDHELD_IP
chmod 755 /home/ark/arkos-nearby-installer
sudo /home/ark/arkos-nearby-installer doctor
sudo /home/ark/arkos-nearby-installer install
```

`doctor` 返回 `supported: true` 后可以安装，`problems` 列出需要处理的配置问题。

## 更新

退出游戏和联机房间，等待原 Wi-Fi 恢复，再退出 Nearby Multiplayer 回到游戏列表。运行新版安装脚本或新版可执行文件的 `install` 即可更新。已有配对凭据、设备身份、记住的设备和原启动文件备份会保留。更新完成后重启掌机，两端使用相同版本再联机。

也可以在设备上执行：

```sh
sudo /opt/arkos-nearby/arkos-nearby update /home/ark/arkos-nearby-installer
```

## 卸载

先退出当前房间和游戏，再执行：

```sh
sudo /opt/arkos-nearby/arkos-nearby uninstall
```

卸载恢复被本工具修改的游戏启动入口，移除本工具的菜单和已记录安装文件。凭据、设备身份、记住的设备、日志和原启动备份保留，以便重新安装或排查。

## 安装内容

运行程序、图形界面、核心查询器、无线资产、许可证与诊断日志位于 `/opt/arkos-nearby/`。Options 入口为 `/opt/system/Nearby Multiplayer.sh` 和 `/opt/system/Nearby Launcher.sh`。安装器接入已识别的 RetroArch 启动脚本 X 分支，以及 EmulationStation 的 FN + X 主页快捷键；原始脚本保存在 `original-launch/`，卸载时恢复。

创建或加入房间时加载联机驱动，结束联机后恢复原驱动和之前连接的 Wi-Fi。仅组网完成后选择「保持连接，返回系统」会保留网络。临时会话文件位于 `/run/arkos-nearby-rust/`，诊断日志保存在 `/opt/arkos-nearby/logs/`。
