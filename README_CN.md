<div align="center">
<img src="branding\logo-2.png" alt="Logo" >

<h3 align="center">
Lecoo控制中心是一个逆向工程的底层嵌入式控制器（EC）守护进程和命令行界面，专为基于Emdoor底盘的笔记本电脑（如Lecoo Pro 14 / Lecoo N155）设计。它提供对散热、功耗限制和灯光的直接硬件级控制，替代了不存在的官方软件。
</h3>
</div>
<div align="center">

[![GitHub Release](https://img.shields.io/github/v/release/LaVashikk/Lecoo-Control-Center?color=orange)](https://github.com/LaVashikk/Lecoo-Control-Center/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/LaVashikk/Lecoo-Control-Center/blob/main/LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-lightgrey.svg)]()
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)]()

🇷🇺 [Russian Readme here](README_RU.md)
CN [中文 Readme 在这](README_RU.md)

</div>

## ⚠️ 重要免责声明

本软件直接与您系统的硬件交互，特别是嵌入式控制器（ITE IT5570/IT8987）的HRAM窗口和底层I/O端口。不正确的配置（例如在高负载下将自定义风扇曲线设置为0 RPM）可能导致过热和不可逆转的硬件损坏。

使用本软件即表示您承认这些风险。作者对您设备造成的任何损坏不承担责任。使用风险自负。

## 功能

  * **系统监控：** 读取CPU/系统温度和风扇速度（RPM）。
  * **电源管理：** 在预定义的EC电源配置文件之间切换（安静、默认、性能）。
  * **温度控制：** 独立管理CPU和GPU风扇（自动、全速或自定义PWM占空比）。
  * **电池健康（FlexiCharger）：** 设置自定义电池充电限制以延长电池寿命（完全、高、平衡、寿命、桌面模式）。
  * **灯光控制：** 调整键盘背光亮度。
  * **后部LED环控制：** 配置后部电源LED的静态亮度或硬件驱动的呼吸动画。

## 支持的硬件

本软件主要在Lecoo Pro 14（Lecoo N155）上开发和测试。不同版本的支持情况如下：

| 型号 | 主板版本 | EC芯片 | 状态 |
| :--- | :--- | :--- | :--- |
| Lecoo Pro 14 Amd (H255) | N155A | IT5571-07 | 确认支持 |
| Lecoo Pro 14 Intel (Core Ultra 5) | N155D | IT5570-02 | 支持除了电源LED控制以外的其他功能 |
| Lecoo Pro 14 Intel (i5-13420H) | N155C | IT5570? | 可能支持 |
| Lecoo Pro 16 / N161A | N161A | IT5570-02 | 仅支持键盘背光 |

**注意：** 本软件理论上可能适用于其他使用ITE IT5570或IT8987嵌入式控制器的基于Emdoor的笔记本电脑，因为守护进程包含基本的HRAM偏移自动检测功能。

如果您成功在未列出的硬件版本或不同的Emdoor底盘上运行此软件，请打开一个issue或联系我以更新兼容性列表！

## 安装

### ⚠️ 推荐：使用预编译二进制文件

**除非您是开发人员，否则请勿从源代码构建！** 请下载最新的预编译版本：

👉 **[下载最新版本](https://github.com/LaVashikk/Lecoo-Control-Center/releases/latest)**

#### Windows 安装

1. 从发布页面下载 `lecoo-*-windows.zip` 压缩包。
2. 将压缩包解压到任意文件夹。
3. 右键点击 `install.bat` 并选择 **"以管理员身份运行"**。
4. 打开新的终端窗口并运行 `lecoo-ctrl help` 以验证安装。

#### Linux 安装

1. 从发布页面下载 `lecoo-*-linux.tar.gz` 压缩包。
2. 解压压缩包：`tar -xzf lecoo-*-linux.tar.gz`
3. 进入解压后的文件夹并运行：`sudo ./install.sh`
4. 使用 `lecoo-ctrl` 命令与守护进程交互。

## 已知问题

* **Windows 11守护进程自动启动：** 后台守护进程目前在Windows 11上无法自动启动。根本原因仍在调查中。
* **电源丢失时FlexiCharger重置：** 如果笔记本电脑关机并从墙上拔下超过5分钟，嵌入式控制器（EC）会清除其内存并重置充电限制。如果您在启动前插上笔记本电脑，电池将充电至100%。但是，一旦系统启动且守护进程初始化，电池会自然放电回您配置的限制并恢复正常行为。
* **自定义LED模式下的充电指示器：** 当后部LED环设置为`custom`模式时，标准电池充电指示器停止工作。
* **硬关机后LED环保持开启：** 如果在后部LED环处于`custom`模式时执行硬关机（按住电源按钮），环将保持点亮。**解决方法：** 打开笔记本电脑并正常关机。
* **与官方软件的冲突：** 使用`power`命令调整TDP配置文件可能与制造商的官方软件（`PowerModeUtility`）冲突。强烈建议一次只使用这些工具中的一个。
* **反作弊软件 (Windows)：** 某些反作弊系统（如 FaceIT）可能会终止守护进程。这是因为守护进程利用官方制造商驱动程序来访问嵌入式控制器。
* **Secure Boot 与内核锁定 (Linux)：** 在启用 Secure Boot 的发行版（如 Fedora）上，由于内核锁定策略，守护进程目前无法访问底层 I/O 端口（`/dev/port`）。此限制将在以后的更新中解决。

## 使用方法（CLI）

守护进程在后台运行。您使用`lecoo-ctrl`命令行工具与其交互。

<img src="branding\cli.jpg" alt="lecoo-ctrl" width=50% >

以下是`lecoo-ctrl`的主要命令：

### 系统信息与监控

  * `lecoo-ctrl help` - 显示可用命令及其用法。
  * `lecoo-ctrl info` - 检索基本EC信息和守护进程版本。
  * `lecoo-ctrl temps` - 显示当前CPU和系统温度。
  * `lecoo-ctrl fans` - 显示当前CPU和GPU风扇速度（RPM）。

### 电源与电池设置

  * `lecoo-ctrl power <silent|default|perf>` - 应用特定的电源/TDP配置文件。
      * *示例：* `lecoo-ctrl power perf`
  * `lecoo-ctrl charge <full|high|balanced|lifespan|desk>` - 设置电池充电阈值（FlexiCharger）。
      * *示例：* `lecoo-ctrl charge desk`（限制充电至40-50%，适用于永久AC使用）。
      * 运行不带参数的`lecoo-ctrl charge`以查看当前限制和电池容量。

### 温度控制

  * `lecoo-ctrl fan <cpu|gpu> <auto|full|custom> [value]` - 控制风扇行为。
      * *示例（自动）：* `lecoo-ctrl fan cpu auto`
      * *示例（最大速度）：* `lecoo-ctrl fan gpu full`
      * *示例（自定义PWM）：* `lecoo-ctrl fan cpu custom 150`（设置自定义占空比）。

### 灯光控制

  * `lecoo-ctrl kbd <0|1|2|3>` - 设置键盘背光级别（0为关闭，3为最大）。
  * `lecoo-ctrl led <auto|custom>` - 控制后部LED环。
      * *示例：* `lecoo-ctrl led custom 50`

## GUI

图形用户界面（GUI）目前正在开发中，将在未来版本中提供。

## 遥测与数据收集

为了帮助改进软件 - 特别是在不同主板版本上完善HRAM自动检测逻辑并捕获意外的守护进程崩溃 - 该项目包含一个**可选的、完全匿名的遥测系统**。

**收集的内容：**

  * 仅微控制器数据（EC芯片版本，HRAM内存偏移）。
  * CPU名称。
  * 基本运行状态（温度，风扇RPM，活动电源配置文件）。
  * 守护进程失败时的崩溃日志（Panic跟踪）。

**不收集的内容：**

  * 绝对不收集操作系统数据、用户名、IP地址、MAC地址或个人信息。

遥测默认启用，以支持项目的发展。如果您希望选择退出，可以随时使用以下命令禁用它：

```bash
lecoo-ctrl daemon telemetry disable
```

## 从源码构建（开发人员）

确保您已安装Rust工具链。

克隆存储库：

```bash
git clone https://github.com/LaVashikk/Lecoo-Control-Center.git
cd Lecoo-Control-Center
```

您可以使用标准Cargo命令构建项目，或使用位于`.cargo/config.toml`中的预定义别名：

**Windows：**

```bash
cargo build-win       # 构建守护进程
cargo build-ctrl-win  # 构建CLI客户端
```

**Linux：**

```bash
cargo build-linux       # 构建守护进程
cargo build-ctrl-linux  # 构建CLI客户端
```

## 许可证与支持

本项目是开源的，根据MIT许可证授权。有关更多详细信息，请参阅[LICENSE](LICENSE)文件。

如果您发现此工具有用并希望支持其持续开发，请考虑请我喝杯咖啡（或啤酒，谁知道呢，哈哈）！

* **国际：** [通过Lava.top捐赠](https://app.lava.top/lavashik?tabId=donate)
* **俄罗斯：** [通过CloudTips捐赠](https://pay.cloudtips.ru/p/7e960f26)
* **中国：** [支付宝](branding/alipay.jpg)
* **加密货币：**
  * **SOL (Solana)：** `CvbAT3VduADYyGRBZDq5CD3kLYcYYjYjFzgWFftsbgAB`
  * **ETH (ERC-20)：** `0x44B03F26B4dc7b8AcBBCFc456e4181872386a8D8`
  * **BTC (Native Segwit)：** `bc1q3sej9r9v9syamjanq7mg6a7002pc4m6d6qnv6k`