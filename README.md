# 妙压（MiaoZip）

妙压（MiaoZip）是一个用 Rust 编写、采用好压经典界面布局的跨平台压缩文件管理器，同一份源码可在 Windows、Linux 和 macOS 上构建运行。

## 功能

- 添加多个文件或目录并创建 ZIP、7z、TAR、TAR.GZ、TAR.BZ2、TAR.XZ、TAR.ZST
- 解压上述七种格式及 RAR；另支持单文件 GZ/BZ2/XZ/ZST 与 TGZ/TBZ/TBZ2/TXZ/TZST 别名。RAR 和单文件压缩格式目前仅支持解压，不支持创建；暂不支持加密压缩包、CAB 等未列出的格式
- 工具箱中的虚拟光驱可只读挂载或卸载 ISO 光盘镜像
- 按参考截图布局的连续蓝色标题栏与工具栏、可展开的真实文件夹树、随选择变化的详细信息栏和磁盘列表；Windows 文件夹图标从系统 Shell 获取
- 地址栏在 Windows 上隐藏 `\\?\` 扩展路径前缀，以常见的磁盘或网络路径显示
- 目录前进/后退、路径跳转、多选、删除确认和文件信息
- 支持把文件拖入窗口
- ZIP 的 0 档为无压缩存储、1–9 档为 Deflate，7z 使用 LZMA2，TAR 变体使用对应压缩算法
- 后台压缩和解压，界面不会因大文件卡死
- 压缩窗口提供蓝色简洁模式与经典参数模式，可随时切换；独立进度窗口显示当前任务、文件及进度
- 工具箱采用图标网格，提供压缩、解压、虚拟光驱、MD5/SHA-256 校验、批量文件改名、批量字符替换、图片转换、ZIP 完整性测试与有限恢复等真实功能入口
- 压缩、解压及设置等弹窗使用独立系统窗口，可拖动到主窗口之外
- 双击压缩文件仅打开主窗口并浏览包内目录；点击“解压到”才打开独立解压窗口
- 双击普通文件时交给操作系统的默认应用；双击包内文件时只将该文件提取到临时目录，再交给默认应用打开（修改临时副本不会写回压缩包）。包内程序或脚本需要再次确认，单项预览上限为 256 MiB
- Windows 文件列表及包内文件按当前系统关联显示原生图标；包内文件按扩展名查询，目录使用系统文件夹图标
- 自动保存最近选择的路径和压缩选项
- 解压前校验路径，阻止越界写入；拒绝符号链接与特殊文件条目
- 自动加载 Windows、macOS 或常见 Linux 发行版的中文系统字体
- Windows 可为实际支持的 13 个压缩文件扩展名注册打开方式，并在明确确认后尝试设为当前用户的默认程序；已有 Windows UserChoice 的格式需在系统“默认应用”页面完成选择
- Windows 可按当前用户注册或移除文件、文件夹的“添加到 ZIP”，各支持格式的“使用妙压解压”和“解压到当前文件夹…”，以及 ISO 的“使用妙压挂载”右键菜单；“解压到当前文件夹…”会先打开确认窗口，不会直接覆盖文件

主界面会显示当前系统的真实磁盘容量和可用空间；因此数值与参考截图不一定相同。密码管理与自解压按钮目前仅保留界面入口，尚未实现相应功能。7z 的“速度”选项暂不调整 LZMA2 压缩级别。

虚拟光驱仅用于 ISO，不把镜像当成普通压缩包解压。Windows 使用系统 Storage 模块，Linux 需要 udisks2 的 `udisksctl` 和可用的桌面授权机制，macOS 使用系统 `hdiutil`。挂载状态只在当前运行会话中跟踪；关闭妙压不会自动卸载。实际挂载需在目标平台及目标镜像上验证；某些混合分区镜像可能需要系统文件管理器进一步操作。

工具箱中的批量改名可按序号、文件名字符替换、前后缀生成预览，执行前会检查重名及现有目标，并要求再次确认。批量字符替换只处理不超过 32 MiB 的 UTF-8 普通文本，结果写入 `.replaced` 新文件；图片转换支持 PNG、JPEG、WebP、BMP 输出，结果写入 `.converted` 新文件，均保留原件；GIF 动图只转换首帧。MD5 仅用于兼容旧校验值，安全敏感场景建议使用同窗口的 SHA-256。ZIP 完整性测试逐项核对大小和 CRC32；“尝试修复”会从仍能读取的 ZIP 中提取校验通过的条目，写入新的 `.recovered.zip`，原包不变，但不能恢复不可读的中央目录、已丢失数据或超过大小限制的内容。这些工具不能判断文件是否安全。云查杀尚未接入第三方服务，程序不会上传文件。

Windows 系统集成入口位于“工具箱”或标题栏右上角菜单，设置窗口分为“综合”和“关联”两页。未注册或仍有格式未设默认时，普通启动会询问是否将妙压设为默认；确认后仅为当前用户更改没有 Windows UserChoice 的格式。已有 UserChoice 的格式会列出，需点击“打开 Windows 默认应用设置”手动切换；程序不会篡改 UserChoice。成功确认后不再重复提示，注册操作也不会自动打开资源管理器或系统设置。右键菜单只写入当前用户的注册表，不需要管理员权限；移动或重命名可执行文件后，可在“综合”页重新注册或修复。“添加到 ZIP”支持多选文件或文件夹，并直接打开一个独立压缩窗口，不弹出主界面；压缩进度与结果也显示在这个窗口。解压和挂载菜单仍只支持单项选择。Windows 11 的经典菜单项通常显示在“显示更多选项”中。双击或通过默认应用打开压缩文件时，仅显示主窗口中的包内列表；选择工具栏“解压到”才进入解压对话框。右键“使用妙压解压”仍直接进入解压对话框；右键打开 ISO 会进入虚拟光驱对话框，确认后才挂载。复合扩展名的 TAR.GZ 等文件可在应用内解压；Windows 根据最后一个扩展名关联 `.tar.gz` 为 `.gz`。注册覆盖 `.zip`、`.7z`、`.rar`、`.tar`、`.gz`、`.bz2`、`.xz`、`.zst`、`.tgz`、`.tbz`、`.tbz2`、`.txz`、`.tzst`，ISO 仅注册右键挂载。

构建时会生成 16、32、48、256 像素的 Windows EXE 图标资源；主窗口及独立弹窗共用同一图标。注册时会把相同的 ICO 写入当前用户的 `%LOCALAPPDATA%\MiaoZip\Icons`，文件名随内容变化，并让右键菜单、各格式的 ProgID `DefaultIcon` 及扩展名的 `DefaultIcon` 指向它，避免 Explorer 长时间缓存旧 EXE 图标。图标取决于 Windows 实际生效的默认处理程序：仅注册候选项不会替换已有 ZIP 图标，显式设为默认且成功后才会使用妙压图标。应用中的默认关联数量读取 Windows Shell 的实际结果，不把“候选已注册”误算成“已设默认”。若菜单图标未刷新，可在“设置 → 综合”重新注册右键菜单。构建脚本使用代码绘制图标，并未从第三方安装包复制图标文件。

部署时也可执行 `miaozip.exe --register-integration` 注册默认应用候选项及右键菜单；明确要设置默认打开方式时执行 `miaozip.exe --set-default-archives`；执行 `miaozip.exe --remove-context-menu` 仅移除妙压的右键菜单。`--register-integration` 和 `--remove-context-menu` 不更改 Windows 已选定的 ZIP 默认应用。

从旧版 ZipDesk 升级时，先用新程序设置默认打开方式并注册右键菜单。妙压只清理能确认属于旧版、且不再被文件关联引用的 ZipDesk 注册项；不会删除旧版可执行文件、用户数据或 Windows 的 UserChoice。若某格式仍由旧版或其他程序处理，可在 Windows 默认应用设置中手动切换。

## 开发环境

先安装当前稳定版 [Rust 工具链](https://rustup.rs/)，然后在项目目录执行：

```bash
cargo run
```

运行测试：

```bash
cargo test
```

构建优化后的可执行文件：

```bash
cargo build --release
```

产物位于 `target/release/`。Windows 下为 `miaozip.exe`，Linux 和 macOS 下为 `miaozip`。

## GitHub Actions 跨平台构建

[构建工作流](.github/workflows/build.yml)会在推送、Pull Request 和手动触发时，对以下目标分别运行测试与 Release 构建，并上传可下载的 Actions Artifact：

| 系统 | 架构 | Rust 目标 |
| --- | --- | --- |
| Windows | x86（32 位） | `i686-pc-windows-msvc` |
| Windows | x64 | `x86_64-pc-windows-msvc` |
| Windows | ARM64 | `aarch64-pc-windows-msvc` |
| Linux | x64 | `x86_64-unknown-linux-gnu` |
| Linux | ARM64 | `aarch64-unknown-linux-gnu` |
| macOS | Intel x64 | `x86_64-apple-darwin` |
| macOS | Apple Silicon ARM64 | `aarch64-apple-darwin` |

每个架构的 Actions Artifact 都包含多种分发形式：

| 系统 | 便携产物 | 安装产物 |
| --- | --- | --- |
| Windows | 独立 `.exe`、`.zip` | `.msi` |
| Linux | 独立二进制、`.tar.gz` | `.deb`、`.rpm` |
| macOS | 独立二进制、`.app.zip` | `.dmg`、`.pkg` |

这些 CI 产物尚未进行商业代码签名或 Apple 公证，因此操作系统可能显示安全提醒。Linux 安装包包含桌面启动项和图标；macOS 的 APP、DMG 与 PKG 使用标准应用包结构。Linux 需要目标系统具备相应桌面和图形运行库；32 位 Linux 与 ARMv7 没有列入此矩阵，避免把尚未验证的 GUI 交叉编译目标标称为可用。

## 各平台准备

### Windows

安装 Rust 时选择默认的 MSVC 工具链，并确保已安装 Visual Studio Build Tools 的“使用 C++ 的桌面开发”组件。

### macOS

安装 Xcode Command Line Tools：

```bash
xcode-select --install
```

### Linux（Debian/Ubuntu）

安装窗口、OpenGL、Wayland/X11 和文件选择器所需的开发库：

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libgl1-mesa-dev \
  libx11-dev libxcursor-dev libxrandr-dev libxi-dev \
  libwayland-dev libxkbcommon-dev libdbus-1-dev
```

桌面环境还应提供 XDG Desktop Portal 或 Zenity，供原生文件选择对话框使用。

## 项目结构

```text
src/
├── main.rs      # 窗口启动和平台入口
├── app.rs       # 应用状态、对话框和后台任务
├── app/shell.rs # 主窗口的经典界面与磁盘信息
├── integration.rs # 启动参数、Windows 默认应用与右键菜单注册
├── optical.rs   # ISO 虚拟光驱跨平台挂载/卸载
└── archive.rs   # 压缩格式创建/解压、安全校验和单元测试
```

跨平台 GUI 使用 `eframe/egui`，系统文件对话框使用 `rfd`，ZIP 读写使用 `zip-rs`，7z 使用 `sevenz-rust2`，RAR 解压使用 `unrar` / UnRAR，TAR 及其压缩变体使用 `tar` 和对应编解码库。UnRAR 的许可说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。各操作系统的最终安装包通常需要在对应系统上生成和签名。
