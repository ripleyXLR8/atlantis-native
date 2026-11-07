<div align="center">

# Atlantis Native

**在今天的電腦上，重返亞特蘭提斯。**

Cryo Interactive 的《亞特蘭提斯：失落的傳說》（*Atlantis: The Lost Tales*，1997）、
《Atlantis II》（1999）與《Atlantis III: The New World》（2001）的原生移植版，
以 Rust 撰寫，支援 Windows、macOS、Linux，以及網頁瀏覽器。《Atlantis III》目前只支援 GOG.com 的法文版。

**[▶ 在瀏覽器中遊玩](https://ripleyxlr8.github.io/atlantis-native/)**（需要你自己的遊戲光碟或 GOG.com 版）

[English](README.md) · [简体中文](README.zh-CN.md) · [下載](#下載) · [需要遊戲本體](#需要遊戲本體) · [快速開始](#快速開始) · [操作方式](#操作方式)

</div>

> 本文件由英文 README 翻譯而成（AI 輔助翻譯）。如有不清楚或錯誤之處，以[英文版](README.md)為準，也歡迎指正。

---

女王的新任侍衛 Seth 在亞特蘭提斯醒來時，女王 Rhea 正好失蹤。Atlantis Native
完整重現整個冒險：360° 全景、過場動畫、嘴型與語音同步的 3D 人物頭像、十七個謎題，
以及片尾字幕，可以從第一個畫面玩到最後一個畫面。

續作《Atlantis II》（*Beyond Atlantis*，1999）同樣可以從頭玩到尾，支援 Windows、macOS、Linux 與瀏覽器：
西藏、愛爾蘭、中國、猶加敦與西巴爾巴、香巴拉與亞特蘭提斯，包含所有謎題（見[玩家指南](docs/GUIDE.md#atlantis-ii)，英文）。

《Atlantis III: The New World》（2001）是 0.6.0 版新增的遊戲，同樣可以從頭玩到尾，支援桌面版與瀏覽器：
從霍加爾（Hoggar）沙漠，經過亡者之地與其他世界，一直到片尾字幕，包含對話、選單、存檔與所有謎題。
目前只知道 GOG.com 的法文版（見[玩家指南](docs/GUIDE.md#atlantis-iii)，英文）。
它主要透過與原版程式碼比對的自動重播測試，以及開發者本人的遊玩來檢驗，玩過的玩家還不多：歡迎回報問題。

這就是**原版遊戲本身，逐像素、逐取樣忠實重現**：引擎移植自
[ScummVM](https://www.scummvm.org/) CryOmni3D 引擎中的 Atlantis 引擎，而後者是透過逆向工程原版執行檔重建的。
每一項行為都對應到原版程式碼中的位址。此外還提供可選的原版錯誤修正與可選的 HD 模式。

## 特色

- 🖥️ **Windows（x64 與 ARM）、macOS（Apple Silicon 與 Intel）、Linux 與網頁**：同一個引擎，到處都是同一款遊戲，甚至可以在瀏覽器分頁中遊玩。
- 🎯 **忠實**：劇本、對話、音樂、3D 頭像與謎題都執行原版自己的邏輯。
- ✨ **HD 模式**（可選）：全景與 3D 頭像以視窗解析度繪製，字幕清晰；開啟**寬螢幕**後，全景會填滿整個寬視窗。
- 🐞 **錯誤修正**（可選）：修正原版的各種錯誤；一個開關即可全部關閉，回到原版出貨時的樣子。
- 💾 **存檔相容**：遊戲在每一章自動存檔，使用原版自己的存檔格式。
- ⚙️ **選項選單**（F10 或 O）：畫面、HD 設定與各項改進，遊玩中即時切換。
- 📚 **遊戲庫**：三款遊戲的多個版本（GOG.com、光碟、DVD、光碟映像檔……）集中在同一個清單中，啟動時選擇要玩的版本，或在 F10 選單中切換。
- 🏜️ **Atlantis III**（0.6.0 版新增）：完整支援 GOG.com 法文版，使用原版自己的選單與選項（640 x 480、800 x 600 或 1024 x 768，16 或 32 位元色彩），存檔不放在遊戲資料夾中，F12 截圖；支援桌面版與瀏覽器。

## 中文版

支援第一部遊戲的兩款中文光碟版：

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
| Windows 11 ARM 版 | `atlantis-native-windows-arm64.zip` |
| macOS 11 以上，Apple Silicon | `atlantis-native-macos-arm64.dmg` |
| macOS 11 以上，Intel | `atlantis-native-macos-x64.dmg` |
| Linux x86-64（glibc 2.35+、ALSA） | `atlantis-native-linux-x64.tar.gz` |
| Linux 64 位元 ARM，例如 Raspberry Pi 4 或 5 | `atlantis-native-linux-arm64.tar.gz` |
| 掌機（[PortMaster](https://portmaster.games/)，64 位元 ARM，如 R36S），第一部遊戲 | `atlantis-native-portmaster.zip`（解壓縮到 `ports` 資料夾，遊戲檔案放進 `ports/atlantisnative/gamedata/`） |
| 網頁（靜態網站，需透過 HTTP 提供；在 nightly 版中） | `atlantis-native-web.zip` |

網頁版：**https://ripleyxlr8.github.io/atlantis-native/**（Chrome、Edge、Firefox 或 Safari），
三款遊戲都可以玩（《Atlantis II》尚未在 Safari 中測試，《Atlantis III》只在 Edge 中測試過）。
在網頁中選擇你的遊戲資料夾即可：檔案只在你的瀏覽器中讀取，不會傳送到任何地方；存檔保存在該瀏覽器中。

> **Windows：** 程式沒有程式碼簽章，第一次執行時 SmartScreen 可能會顯示警告。
> 點選「**其他資訊**」，再點「**仍要執行**」即可。請只從[發行頁面](https://github.com/ripleyXLR8/atlantis-native/releases)下載。

> **macOS：** 程式未經 Apple 公證。第一次開啟後，到「**系統設定 → 隱私權與安全性**」點選「**強制打開**」。

## 需要遊戲本體

Atlantis Native **不含任何遊戲資料**，請使用你自己的正版遊戲（完整清單見[英文 README](README.md#you-need-the-game)）。

- **《Atlantis: The Lost Tales》**：除了兩款中文版，也支援英文、法文、德文、荷蘭文、瑞典文、西班牙文、義大利文、
  捷克文與巴西葡萄牙文光碟版、Cryo 1.01 修正檔、Cryo 的 DVD-ROM（1998）、Perfect Edition、義大利文 *Atlantis Collection* DVD，
  以及 [GOG.com](https://www.gog.com/) 版（英文、法文、德文、西班牙文、義大利文）。
- **《Atlantis II》**：法文光碟版（1.00 版）、官方 1.01 修正檔、北美 DreamCatcher 光碟版（*Beyond Atlantis*，2000）、
  [GOG.com](https://www.gog.com/) 版（英文、法文、德文、西班牙文）與義大利文 *Atlantis Collection* DVD。
- **《Atlantis III》**：只支援 [GOG.com](https://www.gog.com/) 的法文版（*Atlantis 3 - The New World*），
  指定其安裝資料夾（含有 `Game.exe` 與 `Datas` 的資料夾）。其他版本（GOG.com 的其他語言版、零售光碟與 DVD）
  尚屬未知，也未經測試：程式會拒絕其 `Game.exe`，並顯示檔案大小與 MD5，方便在 GitHub 上回報以便加入支援。

## 快速開始

1. 下載並解壓縮適合你系統的版本。
2. 雙擊 **atlantis-native**（macOS 上為 **Atlantis Native**）。
3. 第一次執行時選擇你的遊戲資料夾（或光碟映像檔所在的資料夾），之後會自動記住。
   程式會先找出電腦上已安裝的遊戲；如果有多個版本或多款遊戲，可以在[遊戲庫](docs/GUIDE.md#several-editions-the-game-library)中選擇要玩的版本（啟動時按住 Shift 即可開啟）。

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

《Atlantis II》與《Atlantis III》有各自的操作方式，見玩家指南中的
[Atlantis II](docs/GUIDE.md#controls) 與 [Atlantis III](docs/GUIDE.md#atlantis-iii-controls)（英文）。

## 關於 AI 的使用

Atlantis Native 大部分程式碼由 AI（Anthropic 的 Claude，透過 Claude Code）撰寫，
並由維護者指導與審閱。引擎依照逆向工程得到的原版程式碼實作，並以原版程式碼在模擬器中的執行結果逐項比對驗證。
歡迎回報任何與原版不同之處。

## 授權

GNU General Public License 第 3 版或更新版本（見 `LICENSE`）。程式碼衍生自 ScummVM 的 CryOmni3D 引擎，
版權所有 (C) ScummVM 團隊。中文字幕字型為 Noto Serif TC / SC 子集（SIL Open Font License 1.1，見
`assets/fonts/LICENSE-NotoSerifCJK`）；Noto 為 Google Inc. 的商標。

*Atlantis: The Lost Tales*、*Atlantis II* 與 *Atlantis III* 為其所有者的商標。本專案與其無任何關聯。
