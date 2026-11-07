<div align="center">

# Atlantis Native

**在今天的电脑上，重返亚特兰蒂斯。**

Cryo Interactive 的《亚特兰蒂斯：失落的传说》（*Atlantis: The Lost Tales*，1997）、
《Atlantis II》（1999）和《Atlantis III: The New World》（2001）的原生移植版，
用 Rust 编写，支持 Windows、macOS、Linux 以及网页浏览器。《Atlantis III》目前只支持 GOG.com 的法文版。

**[▶ 在浏览器中游玩](https://ripleyxlr8.github.io/atlantis-native/)**（需要你自己的游戏光盘或 GOG.com 版）

[English](README.md) · [繁體中文](README.zh-TW.md) · [下载](#下载) · [需要游戏本体](#需要游戏本体) · [快速开始](#快速开始) · [操作方式](#操作方式)

</div>

> 本文件译自英文 README（AI 辅助翻译）。如有不清楚或错误之处，以[英文版](README.md)为准，也欢迎指正。

---

女王的新任侍卫 Seth 在亚特兰蒂斯醒来时，女王 Rhea 恰好失踪。Atlantis Native
完整重现整个冒险：360° 全景、过场动画、口型与语音同步的 3D 人物头像、十七个谜题，
以及片尾字幕，可以从第一个画面玩到最后一个画面。

续作《Atlantis II》（*Beyond Atlantis*，1999）同样可以从头玩到尾，支持 Windows、macOS、Linux 和浏览器：
西藏、爱尔兰、中国、尤卡坦与西巴尔巴、香巴拉和亚特兰蒂斯，包含所有谜题（见[玩家指南](docs/GUIDE.md#atlantis-ii)，英文）。

《Atlantis III: The New World》（2001）是 0.6.0 版新增的游戏，同样可以从头玩到尾，支持桌面版和浏览器：
从霍加尔（Hoggar）沙漠，经过亡者之地和其他世界，一直到片尾字幕，包含对话、菜单、存档和所有谜题。
目前只知道 GOG.com 的法文版（见[玩家指南](docs/GUIDE.md#atlantis-iii)，英文）。
它主要通过与原版代码比对的自动回放测试，以及开发者本人的游玩来检验，玩过的玩家还不多：欢迎回报问题。

这就是**原版游戏本身，逐像素、逐采样忠实重现**：引擎移植自
[ScummVM](https://www.scummvm.org/) CryOmni3D 引擎中的 Atlantis 引擎，而后者是通过逆向工程原版可执行文件重建的。
每一项行为都对应到原版代码中的地址。此外还提供可选的原版错误修复和可选的 HD 模式。

## 特色

- 🖥️ **Windows（x64 和 ARM）、macOS（Apple Silicon 和 Intel）、Linux 和网页**：同一个引擎，到处都是同一款游戏，甚至可以在浏览器标签页中游玩。
- 🎯 **忠实**：剧本、对话、音乐、3D 头像和谜题都运行原版自己的逻辑。
- ✨ **HD 模式**（可选）：全景和 3D 头像以窗口分辨率绘制，字幕清晰；打开**宽屏**后，全景会填满整个宽窗口。
- 🐞 **错误修复**（可选）：修复原版的各种错误；一个开关即可全部关闭，回到原版发行时的样子。
- 💾 **存档兼容**：游戏在每一章自动存档，使用原版自己的存档格式。
- ⚙️ **选项菜单**（F10 或 O）：画面、HD 设置和各项改进，游戏中即时切换。
- 📚 **游戏库**：三款游戏的多个版本（GOG.com、光盘、DVD、光盘镜像……）集中在同一个列表中，启动时选择要玩的版本，或在 F10 菜单中切换。
- 🏜️ **Atlantis III**（0.6.0 版新增）：完整支持 GOG.com 法文版，使用原版自己的菜单和选项（640 x 480、800 x 600 或 1024 x 768，16 或 32 位色），存档不放在游戏文件夹中，F12 截图；支持桌面版和浏览器。

## 中文版

支持第一部游戏的两款中文光盘版：

| 版本 | 内容 | 要指定的位置 |
|---|---|---|
| 繁体中文光盘版（1998 年版，4 张光盘） | 繁体中文字幕和菜单，英文语音 | 包含 `atlantis.exe` 和四个 `BIGCD` 文件的文件夹，或四个光盘镜像文件 |
| 简体中文光盘版（1999 年版，4 张光盘） | 简体中文字幕（菜单为原版附带的繁体中文图片），英文语音 | 同上 |

字幕字体：如果你把 Windows 的 `simsun.ttc`（简体）或 `mingliu.ttc`（繁体）放进游戏文件夹，
就会使用这些原版所用的字体；否则使用内置的 Noto Serif SC 或 TC 字体（与思源宋体同一设计，SIL Open Font License 1.1）。

菜单、章节列表和片头字幕使用原版附带的中文图片，与原版完全相同。

## 下载

[最新正式版](https://github.com/ripleyXLR8/atlantis-native/releases/latest)提供当前版本的构建文件；
[nightly 版](https://github.com/ripleyXLR8/atlantis-native/releases/tag/nightly)则是最新提交的构建文件：

| 系统 | 文件 |
|---|---|
| Windows 10 及以上，64 位 | `atlantis-native-windows-x64.zip` |
| Windows 11 ARM 版 | `atlantis-native-windows-arm64.zip` |
| macOS 11 及以上，Apple Silicon | `atlantis-native-macos-arm64.dmg` |
| macOS 11 及以上，Intel | `atlantis-native-macos-x64.dmg` |
| Linux x86-64（glibc 2.35+、ALSA） | `atlantis-native-linux-x64.tar.gz` |
| Linux 64 位 ARM，例如 Raspberry Pi 4 或 5 | `atlantis-native-linux-arm64.tar.gz` |
| 掌机（[PortMaster](https://portmaster.games/)，64 位 ARM，如 R36S），第一部游戏 | `atlantis-native-portmaster.zip`（解压到 `ports` 文件夹，游戏文件放进 `ports/atlantisnative/gamedata/`） |
| 网页（静态网站，需通过 HTTP 提供；在 nightly 版中） | `atlantis-native-web.zip` |

网页版：**https://ripleyxlr8.github.io/atlantis-native/**（Chrome、Edge、Firefox 或 Safari），
三款游戏都可以玩（《Atlantis II》尚未在 Safari 中测试，《Atlantis III》只在 Edge 中测试过）。
在网页中选择你的游戏文件夹即可：文件只在你的浏览器中读取，不会发送到任何地方；存档保存在该浏览器中。

> **Windows：** 程序没有代码签名，第一次运行时 SmartScreen 可能会显示警告。
> 点击“**更多信息**”，再点“**仍要运行**”即可。请只从[发布页面](https://github.com/ripleyXLR8/atlantis-native/releases)下载。

> **macOS：** 程序未经 Apple 公证。第一次打开后，到“**系统设置 → 隐私与安全性**”点击“**仍要打开**”。

## 需要游戏本体

Atlantis Native **不含任何游戏数据**，请使用你自己的正版游戏（完整列表见[英文 README](README.md#you-need-the-game)）。

- **《Atlantis: The Lost Tales》**：除了两款中文版，还支持英文、法文、德文、荷兰文、瑞典文、西班牙文、意大利文、
  捷克文和巴西葡萄牙文光盘版、Cryo 1.01 补丁、Cryo 的 DVD-ROM（1998）、Perfect Edition、意大利文 *Atlantis Collection* DVD，
  以及 [GOG.com](https://www.gog.com/) 版（英文、法文、德文、西班牙文、意大利文）。
- **《Atlantis II》**：法文光盘版（1.00 版）、官方 1.01 补丁、北美 DreamCatcher 光盘版（*Beyond Atlantis*，2000）、
  [GOG.com](https://www.gog.com/) 版（英文、法文、德文、西班牙文）和意大利文 *Atlantis Collection* DVD。
- **《Atlantis III》**：只支持 [GOG.com](https://www.gog.com/) 的法文版（*Atlantis 3 - The New World*），
  指定其安装文件夹（包含 `Game.exe` 和 `Datas` 的文件夹）。其他版本（GOG.com 的其他语言版、零售光盘和 DVD）
  尚属未知，也未经测试：程序会拒绝其 `Game.exe`，并显示文件大小和 MD5，方便在 GitHub 上回报以便加入支持。

## 快速开始

1. 下载并解压适合你系统的版本。
2. 双击 **atlantis-native**（macOS 上为 **Atlantis Native**）。
3. 第一次运行时选择你的游戏文件夹（或光盘镜像所在的文件夹），之后会自动记住。
   程序会先找出电脑上已安装的游戏；如果有多个版本或多款游戏，可以在[游戏库](docs/GUIDE.md#several-editions-the-game-library)中选择要玩的版本（启动时按住 Shift 即可打开）。

也可以在终端中指定文件夹：`atlantis-native /游戏/文件夹`。

## 操作方式

| 动作 | 操作 |
|---|---|
| 环顾四周 | 移动鼠标，或方向键 |
| 前往、交谈、使用 | 在光标有动画的地方点击左键 |
| 下一句对话 | 左键 |
| 跳过人物对话 | Escape |
| 物品栏 | 右键 |
| 游戏菜单（读取、选项、退出） | 在全景画面按 Escape |
| **移植版选项**（画面、HD、错误修复、游戏库） | **F10**（或 **O**） |
| 全屏 | F、F11 或 Alt+Enter |

《Atlantis II》和《Atlantis III》有各自的操作方式，见玩家指南中的
[Atlantis II](docs/GUIDE.md#controls) 和 [Atlantis III](docs/GUIDE.md#atlantis-iii-controls)（英文）。

## 关于 AI 的使用

Atlantis Native 大部分代码由 AI（Anthropic 的 Claude，通过 Claude Code）编写，
并由维护者指导和审阅。引擎按照逆向工程得到的原版代码实现，并用原版代码在模拟器中的运行结果逐项比对验证。
欢迎报告任何与原版不同之处。

## 授权

GNU General Public License 第 3 版或更高版本（见 `LICENSE`）。代码衍生自 ScummVM 的 CryOmni3D 引擎，
版权所有 (C) ScummVM 团队。中文字幕字体为 Noto Serif SC / TC 子集（SIL Open Font License 1.1，见
`assets/fonts/LICENSE-NotoSerifCJK`）；Noto 是 Google Inc. 的商标。

*Atlantis: The Lost Tales*、*Atlantis II* 和 *Atlantis III* 是其所有者的商标。本项目与其没有任何关联。
