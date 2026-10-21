# GPU 界面构建

发行版通过 SDL2 的 GLES2 渲染器显示菜单，静态链接 SDL2 2.32.10。EGL、GBM、DRM、PulseAudio 和 ALSA 库由设备提供。软件渲染和 GPU 渲染共享页面、显示协议与手柄输入。

mirui 0.51.0 在单采样 GLES 配置下保留帧渲染目标，按损伤区域更新画面。截图通过框架的像素读取接口导出。

## 构建环境

需要 Rust 1.92、`aarch64-unknown-linux-gnu` 标准库、ARM64 GNU C 链接器及与设备兼容的 sysroot。SDL2 源码随 Release 提供；锁定的 Rust 依赖位于 `arkos-nearby-0.9.1-rust-sources.tar.gz`。

使用现有的 ARM64 CMake 工具链文件构建 SDL2。工具链应设置 Linux、aarch64、交叉编译器和 sysroot，并将库、头文件及包查询限制到 sysroot。sysroot 需包含 PulseAudio 和 ALSA 开发头文件与库。构建结果中的 `SDL_config.h` 应定义 `SDL_AUDIO_DRIVER_PULSEAUDIO` 和 `SDL_AUDIO_DRIVER_ALSA`，动态库名分别为 `libpulse.so.0` 和 `libasound.so.2`。

```sh
cmake -S /path/to/SDL2-2.32.10 -B /tmp/nearby-sdl \
  -DCMAKE_TOOLCHAIN_FILE=/path/to/aarch64-toolchain.cmake \
  -DCMAKE_BUILD_TYPE=Release -DSDL_SHARED=OFF -DSDL_STATIC=ON \
  -DSDL_KMSDRM=ON -DSDL_KMSDRM_SHARED=ON -DSDL_OPENGLES=ON \
  -DSDL_OPENGL=OFF -DSDL_X11=OFF -DSDL_WAYLAND=OFF \
  -DSDL_ALSA=ON -DSDL_ALSA_SHARED=ON -DSDL_DUMMYAUDIO=OFF \
  -DSDL_PULSEAUDIO=ON -DSDL_PULSEAUDIO_SHARED=ON \
  -DSDL_JACK=OFF -DSDL_PIPEWIRE=OFF
cmake --build /tmp/nearby-sdl --parallel

CARGO_TARGET_DIR=/tmp/arkos-rust-build/public/gpu-static \
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=/path/to/aarch64-linux-gnu-clang \
RUSTFLAGS='-L native=/tmp/nearby-sdl' \
cargo +1.92 build --manifest-path gui/nearby/Cargo.toml \
  --offline --locked --release --target aarch64-unknown-linux-gnu --features gpu
```

界面程序生成在 `/tmp/arkos-rust-build/public/gpu-static/aarch64-unknown-linux-gnu/release/arkos-nearby-gui`。发行资产中的 `nearby-gui-build.json` 记录界面源码、锁文件、框架包、SDL2 静态库及程序的摘要。

## 动态链接构建

动态链接版本通过现有 ARM64 SDL2 库构建，库所在目录需要提供 `libSDL2.so` 链接名。

```sh
python3 scripts/build_nearby_gpu.py check --sysroot /path/to/sysroot --sdl-library /path/to/libSDL2-2.0.so.0 --linker /path/to/aarch64-linux-gnu-clang
python3 scripts/build_nearby_gpu.py build --sysroot /path/to/sysroot --sdl-library /path/to/libSDL2-2.0.so.0 --linker /path/to/aarch64-linux-gnu-clang
```

输出位于 `dist/gpu/`，包含界面程序、私有 SDL2 库、许可证和构建记录。运行时将 `LD_LIBRARY_PATH` 指向该目录的 `lib/`。

## 实机检查

在掌机图形前端释放显示设备后，执行以下命令检查静态链接的发行程序。

```sh
SDL_VIDEODRIVER=kmsdrm SDL_INPUT_LINUX_KEEP_KBD=1 /path/to/arkos-nearby-gui --gpu-probe /tmp/nearby-gpu.png
```

检查程序要求 SDL 渲染器为 `opengles2`，绘制模式选择页，并在交换缓冲区前保存 PNG。标准输出包含渲染器、首帧耗时和截图字节数。检查结束后恢复前端；正式菜单运行日志包含渲染后端与帧耗时统计。

传入显示状态 JSON 可检查指定页面：`arkos-nearby-gui --gpu-probe /path/to/view.json /tmp/nearby-gpu.png`。该命令使用设备的 GPU 后端绘制并返回渲染错误，可用于核对页面的实际输出。

菜单启动器设置 `SDL_INPUT_LINUX_KEEP_KBD=1`，让手柄映射生成的按键继续进入 TTY 输入通道。SDL 保留事件轮询，页面操作仍通过既有显示协议发送给房间管理器。

`gpu` 构建启用 `sound`，通过 MIRUI 的 `AudioPlugin` 和 `SdlAudioSink` 实时合成菜单音效。固定音符存于 `AudioBank`，音频在 SDL 回调中混音。SDL 优先连接系统 PulseAudio 服务，菜单进程退出时释放音频设备。单独检查音频可运行 `arkos-nearby-gui --audio-probe`，它会播放一次确认音并输出音频设备状态；实际听感仍需通过掌机扬声器确认。

音效由菜单的 `SoundState` Model 管理。输入和显示状态通过 Model 方法更新，触发与去重策略产生 `Cue`，`#[effects]` 提取器取出事件，`on_effect` 消费者通过 `AudioHandle` 提交播放命令。框架在每次 Model 事务完成后交付事件，保持输入顺序；音频插件在 `pre_render` 中处理命令。页面切换复用 Model，退出菜单时随 App 释放；音频输出状态使用即时读取。

显示检查结束后恢复前端，再检查手柄操作与退出行为。安装记录管理界面程序及其构建身份。


## 界面状态与动画

DisplayState 管理房间和游戏列表、展示文案及动画状态。列表使用 ModelList 保存行身份，信号条直接观察对应行的索引、强度和可用状态；文案通过命名变更订阅独立更新。动画系统在 MIRUI 的 Animation 调度阶段读取 MonoClock，推进 Tween 和页面动作。截图与演示动画注入确定性时钟，执行相同的系统。

Settings Model 管理主题、静音和音量。主题选择通过 App::set_theme 应用，变更事件负责确认音与偏好保存；音频变更事件更新 AudioHandle。设置恢复保持静默，文件格式沿用 ui-theme 和 ui-sound.json。
