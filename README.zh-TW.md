<div align="center">

# Atlantis Native

**在今天的電腦上，重返亞特蘭提斯。**

Cryo Interactive 1997 年作品《亞特蘭提斯：失落的傳說》（*Atlantis: The Lost Tales*）的原生移植版，
以 Rust 撰寫，支援 Windows、macOS、Linux，以及網頁瀏覽器。

**[▶ 在瀏覽器中遊玩](https://ripleyxlr8.github.io/atlantis-native/)**（需要你自己的遊戲光碟或 GOG 版）

[English](README.md) · [简体中文](README.zh-CN.md) · [下載](#下載) · [需要遊戲本體](#需要遊戲本體) · [快速開始](#快速開始) · [操作方式](#操作方式)

</div>

> 本文件由英文 README 翻譯而成（AI 輔助翻譯）。如有不清楚或錯誤之處，以[英文版](README.md)為準，也歡迎指正。

---

女王的新任侍衛 Seth 在亞特蘭提斯醒來時，女王 Rhea 正好失蹤。Atlantis Native
完整重現整個冒險：360° 全景、過場動畫、嘴型與語音同步的 3D 人物頭像、十七個謎題，
以及片尾字幕，可以從第一個畫面玩到最後一個畫面。

這就是**原版遊戲本身，逐像素、逐取樣忠實重現**：引擎移植自
[ScummVM](https://www.scummvm.org/) CryOmni3D 引擎中的 Atlantis 引擎，而後者是透過逆向工程原版執行檔重建的。
每一項行為都對應到原版程式碼中的位址。此外還提供可選的原版錯誤修正與可選的 HD 模式。

## 特色

- 🖥️ **Windows、macOS（Apple Silicon）、Linux 與網頁**：同一個引擎，到處都是同一款遊戲，甚至可以在瀏覽器分頁中遊玩。
- 🎯 **忠實**：劇本、對話、音樂、3D 頭像與謎題都執行原版自己的邏輯。
- ✨ **HD 模式**（可選）：全景與 3D 頭像以視窗解析度繪製，字幕清晰；開啟**寬螢幕**後，全景會填滿整個寬視窗。
- 🐞 **錯誤修正**（可選）：修正原版的各種錯誤；一個開關即可全部關閉，回到原版出貨時的樣子。
- 💾 **存檔相容**：遊戲在每一章自動存檔，使用原版自己的存檔格式。
- ⚙️ **選項選單**（F10 或 O）：畫面、HD 設定與各項改進，遊玩中即時切換。
- 📚 **遊戲庫**：多個版本（GOG.com、光碟、光碟映像檔……）集中在同一個清單中選擇。

## 中文版

支援兩款中文光碟版：

| 版本 | 內容 | 要指定的位置 |
|---|---|---|
| 繁體中文光碟版（1998 年版，4 片光碟） | 繁體中文字幕與選單，英文語音 | 含有 `atlantis.exe` 與四個 `BIGCD` 檔案的資料夾，或四個光碟映像檔 |
| 簡體中文光碟版（1999 年版，4 片光碟） | 簡體中文字幕（選單為原版附帶的繁體中文圖片），英文語音 | 同上 |

字幕字型：如果你把 Windows 的 `mingliu.ttc`（繁體）或 `simsun.ttc`（簡體）放進遊戲資料夾，
就會使用這些原版所用的字型；否則使用內建的 Noto Serif TC 或 SC 字型（與思源宋體同一設計，SIL Open Font License 1.1）。

選單、章節清單與片頭字幕使用原版附帶的中文圖片，與原版完全相同。

## 下載

[最新正式版](https://github.com/ripleyXLR8/atlantis-native/releases/latest)提供目前版本的建置檔；
[nightly 版](https://github.com/ripleyXLR8/atlantis-native/releases/tag/nightly)則是最新提交的建置檔：

| 系統 | 檔案 |
|---|---|
| Windows 10 以上，64 位元 | `atlantis-native-windows-x64.zip` |
| macOS 11 以上，Apple Silicon | `atlantis-native-macos-arm64.dmg` |
| Linux x86-64（glibc 2.35+、ALSA） | `atlantis-native-linux-x64.tar.gz` |
| Linux 64 位元 ARM，例如 Raspberry Pi 4 或 5 | `atlantis-native-linux-arm64.tar.gz` |

網頁版：**https://ripleyxlr8.github.io/atlantis-native/**（Chrome、Edge、Firefox 或 Safari）。
在網頁中選擇你的遊戲資料夾即可：檔案只在你的瀏覽器中讀取，不會傳送到任何地方。

> **Windows：** 程式沒有程式碼簽章，第一次執行時 SmartScreen 可能會顯示警告。
> 點選「**其他資訊**」，再點「**仍要執行**」即可。請只從[發行頁面](https://github.com/ripleyXLR8/atlantis-native/releases)下載。

> **macOS：** 程式未經 Apple 公證。第一次開啟後，到「**系統設定 → 隱私權與安全性**」點選「**強制打開**」。

## 需要遊戲本體

Atlantis Native **不含任何遊戲資料**，請使用你自己的正版遊戲。
除了兩款中文版，也支援英文、法文、德文、荷蘭文、瑞典文、西班牙文、義大利文光碟版、
Cryo 1.01 修正檔，以及 [GOG.com](https://www.gog.com/) 版（完整清單見[英文 README](README.md#you-need-the-game)）。

## 快速開始

1. 下載並解壓縮適合你系統的版本。
2. 雙擊 **atlantis-native**（macOS 上為 **Atlantis Native**）。
3. 第一次執行時選擇你的遊戲資料夾（或光碟映像檔所在的資料夾），之後會自動記住。

也可以在終端機中指定資料夾：`atlantis-native /遊戲/資料夾`。

## 操作方式

| 動作 | 操作 |
|---|---|
| 環顧四周 | 移動滑鼠，或方向鍵 |
| 前往、交談、使用 | 在游標有動畫的地方按左鍵 |
| 下一句對話 | 左鍵 |
| 跳過人物對話 | Escape |
| 物品欄 | 右鍵 |
| 遊戲選單（讀取、選項、離開） | 在全景畫面按 Escape |
| **移植版選項**（畫面、HD、錯誤修正、遊戲庫） | **F10**（或 **O**） |
| 全螢幕 | F、F11 或 Alt+Enter |

## 關於 AI 的使用

Atlantis Native 大部分程式碼由 AI（Anthropic 的 Claude，透過 Claude Code）撰寫，
並由維護者指導與審閱。引擎依照逆向工程得到的原版程式碼實作，並以原版程式碼在模擬器中的執行結果逐項比對驗證。
歡迎回報任何與原版不同之處。

## 授權

GNU General Public License 第 3 版或更新版本（見 `LICENSE`）。程式碼衍生自 ScummVM 的 CryOmni3D 引擎，
版權所有 (C) ScummVM 團隊。中文字幕字型為 Noto Serif TC / SC 子集（SIL Open Font License 1.1，見
`assets/fonts/LICENSE-NotoSerifCJK`）；Noto 為 Google Inc. 的商標。

*Atlantis: The Lost Tales* 為其所有者的商標。本專案與其無任何關聯。
