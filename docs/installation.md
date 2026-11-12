# 安装、更新与卸载

Howff 适用于 R36S 的 ArkOS4Clone 配置，具体内核、无线芯片和系统要求见 [设备兼容性](compatibility.md#设备配置)。两台掌机安装同一版本后即可联机。

## 下载安装包

打开 [GitHub Releases](https://github.com/W-Mai/arkos-nearby/releases)，选择要安装的版本，下载该版本的 `howff-VERSION-arm64.tar.gz` 和 `SHA256SUMS`。`VERSION` 是版本号。

在下载目录核对安装包的 SHA-256 与 `SHA256SUMS` 中同名文件的记录：

```sh
# Linux
sha256sum howff-*-arm64.tar.gz
# macOS
shasum -a 256 howff-*-arm64.tar.gz
```

解压得到 `howff` 文件夹，其中有五个文件：

| 文件 | 用途 |
| --- | --- |
| `howff` | 安装程序与运行程序 |
| `Install Howff.sh` | 掌机安装入口 |
| `README.md` | 安装与使用说明 |
| `SHA256SUMS` | 可执行文件的 SHA-256 |
| `manifest.json` | 版本、设备适配与程序摘要 |

## 在掌机安装

1. 将解压出的整个 `howff` 文件夹复制到游戏卡，例如放在 `ports/howff/`。
2. 在掌机文件管理器中打开这个文件夹，运行 `Install Howff.sh`。
3. 按安装器提示完成安装。安装器会检查内核、无线芯片、屏幕与系统工具，并接入系统启动入口。
4. 安装完成后，在 **Options → Howff Launcher** 打开收藏首页，或在 **Options → Howff Nearby** 打开附近联机。

系统主页按 **FN + X** 打开附近联机；从系统游戏列表启动游戏时按住 **X**，打开当前游戏的联机面板。完整操作见 [使用说明](usage.md)。

## 在界面中更新

1. 下载所需版本的 `howff-VERSION-arm64.tar.gz`，核对 SHA-256 后，将归档复制到游戏卡。
2. 结束游戏和房间，等待普通 Wi-Fi 恢复。
3. 打开 Howff，按 **START → 应用管理 → 安装更新**。
4. 浏览游戏卡，选中安装包。页面显示当前版本、安装包版本和适配结果。
5. 选择 **安装此版本** 并确认，等待完成，再按画面提示继续进入应用。

安装较早版本也使用这个流程，选择对应版本的安装包即可。更新后，在 **应用管理 → 版本信息** 查看版本；将两台掌机更新到同一版本后再联机。

更新和重复安装会保留设备名称、身份与好友、收藏、最近游玩、偏好和存档备份。数据位置见 [数据与存档](data.md)。也可以解压新版安装包，再运行其中的 `Install Howff.sh` 更新。

## 通过 SSH 安装或更新

在掌机 Options 中开启远程服务，查看当前 IP。以下命令中的 `HANDHELD_IP` 替换为这个地址；在电脑上进入安装包解压后的文件夹，上传其中的 `howff` 可执行文件，再连接掌机：

```sh
scp howff ark@HANDHELD_IP:/home/ark/howff-installer
ssh ark@HANDHELD_IP
chmod 755 /home/ark/howff-installer
sudo /home/ark/howff-installer doctor
```

`doctor` 的 `supported: true` 表示设备检查通过，`problems` 列出检查结果中的问题。接下来按安装情况选择命令。

首次安装，在掌机 SSH 会话中执行：

```sh
sudo /home/ark/howff-installer install
```

更新现有安装时，先上传新版 `howff`，结束游戏和房间、等待 Wi-Fi 恢复，并退出正在运行的 Howff 界面，再在掌机 SSH 会话中执行：

```sh
sudo /opt/howff/howff update /home/ark/howff-installer
```

命令行 `update` 读取可执行文件；界面中的「安装更新」读取 `.tar.gz` 安装包。

## 卸载

结束游戏和房间，等待 Wi-Fi 恢复，退出 Howff 后，通过 SSH 执行：

```sh
sudo /opt/howff/howff uninstall
```

卸载会恢复原系统启动脚本，移除 Howff 的 Options 入口和安装记录中的程序文件。个人数据、偏好、存档备份、日志与原入口备份保留在 `/opt/howff/`，重新安装后可以继续使用。

系统主页会重新载入，释放 Howff 的快捷键监听器和输入过滤器。

## 安装位置

| 内容 | 位置 |
| --- | --- |
| 程序、界面、核心与无线组件 | `/opt/howff/` |
| 启动器入口 | `/opt/system/Howff Launcher.sh` |
| 附近联机入口 | `/opt/system/Howff Nearby.sh` |
| 原系统启动脚本备份 | `/opt/howff/original-launch/` |
| 诊断日志 | `/opt/howff/logs/` |
| 临时运行与联机会话 | `/run/howff/` |

组件许可证随程序安装到 `/opt/howff/`，来源见 [第三方组件](third-party.md)。
