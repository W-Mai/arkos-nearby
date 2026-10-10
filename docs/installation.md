# 安装、更新与卸载

## 下载与校验

两台掌机安装同一个版本。下载 [arkos-nearby-0.3.0-arm64.tar.gz](https://github.com/W-Mai/arkos-nearby/releases/download/v0.3.0/arkos-nearby-0.3.0-arm64.tar.gz) 和 [SHA256SUMS](https://github.com/W-Mai/arkos-nearby/releases/download/v0.3.0/SHA256SUMS)。在下载目录执行 `sha256sum --ignore-missing -c SHA256SUMS`；macOS 可使用 `shasum -a 256 -c SHA256SUMS` 并只核对实际下载的文件。

解压后包含 `arkos-nearby`、`Install Nearby Multiplayer.sh`、安装说明、许可证和可执行文件校验值。

## 从游戏卡安装

将解压出的 `arkos-nearby` 文件夹复制到游戏卡的 `ports` 目录。在掌机的 Ports 中打开 `Install Nearby Multiplayer.sh`。程序与脚本必须放在同一目录。安装器检查实际内核、无线芯片、屏幕、系统工具和启动脚本结构，通过后安装应用并添加手动入口。

安装完成后，Options 中的 Nearby Multiplayer 打开图形游戏选择器。也可以在游戏开始加载前按住 X，进入当前游戏的联机界面。

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

退出游戏和联机房间，等待原 Wi-Fi 恢复。运行新版安装脚本或新版可执行文件的 `install` 即可更新。已有配对凭据、设备身份、记住的设备和原启动文件备份会保留。两端更新到相同版本后再联机。

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

运行程序、图形界面、核心查询器、无线资产、许可证与诊断日志位于 `/opt/arkos-nearby/`。Options 入口为 `/opt/system/Nearby Multiplayer.sh`。安装器修改已识别的 `/usr/local/bin/retroarch` 和 `retroarch32` 启动脚本中的 X 分支，原始字节保存在 `original-launch/`。

创建或加入房间时加载联机驱动，退出后恢复原驱动和之前连接的 Wi-Fi。临时会话文件位于 `/run/arkos-nearby-rust/`，诊断日志保存在 `/opt/arkos-nearby/logs/`。
