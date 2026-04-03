# KoGames 🎮

**[English](#english) | [中文](#中文)**

![KoGames](https://img.shields.io/badge/KoGames-Game%20Library%20Manager-6366f1?style=for-the-badge)
![License](https://img.shields.io/badge/license-MIT-green?style=for-the-badge)
![Platform](https://img.shields.io/badge/platform-Windows-blue?style=for-the-badge)

---

## 中文

**KoGames** 是一个开源的游戏库管理器，专为大屏/手柄模式设计，支持自动导入 Steam、Epic、EA、Ubisoft、Xbox、GOG 等平台的游戏，并提供统一的启动与管理体验。

### ✨ 功能特色

- 🖥️ **双模式 UI** — 全屏大屏模式 + 传统桌面模式，一键切换
- 🎮 **7 大平台导入** — Steam / Epic / EA / Ubisoft / Xbox / GOG / Battle.net
- 📊 **深度统计** — 游戏时长、通关状态、平台分布、年度回顾、成就里程碑
- 🔍 **智能搜索** — 支持按名称、开发商、发行商、标签、类型搜索
- 📌 **快捷启动** — 固定常玩游戏到首页
- 🎬 **Steam 视频预览** — 自动获取 Steam 游戏视频和封面
- 🖼️ **SteamGridDB 集成** — 非 Steam 游戏也能自动获取高质量封面
- 🌐 **IGDB 元数据** — 自动获取开发商、发行商、评分、简介
- 🏷️ **标签系统** — 自定义标签，按标签筛选和分组
- 🔋 **电池状态** — 笔记本/掌机电量实时显示
- 🌙 **待机屏保** — 2 分钟无操作自动进入时钟屏保
- 🌐 **中英文切换** — 自动检测系统语言，支持手动切换
- 💾 **备份/恢复** — ZIP 备份整个游戏库，支持自动备份
- 🎲 **随机选游戏** — 不知道玩什么？一键随机
- 📝 **游戏备注** — 为游戏添加个人笔记
- ⌨️ **快捷键** — Ctrl+F 搜索、Ctrl+A 全选、F11 切换模式
- 📁 **导入导出** — JSON 格式导入导出游戏库
- 🎯 **Pre/Post 脚本** — 游戏启动前后自动执行脚本
- 🏆 **成就系统** — 游玩里程碑自动解锁
- 📅 **年度回顾** — 类似 Steam Replay 的年度统计

### 🚀 快速开始

#### 前置依赖

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/)

#### 开发模式

```bash
# 安装依赖
npm install

# 启动热重载开发
dev.bat
# 或手动运行
npm run tauri:dev
```

#### 构建安装包

```bash
# 构建 exe 安装包
build.bat
# 安装包将输出到 release/ 文件夹
```

### 📦 技术栈

- **前端**: Svelte 5 + TypeScript + Vite
- **后端**: Rust + Tauri 2
- **UI**: 自定义大屏主机风格界面

### ⭐ 觉得好用？

如果 KoGames 对你有帮助，请给个 **Star** ⭐ 支持一下！

[👉 点击这里给个 Star](https://github.com/kobolingfeng/KoGames)

### ❤️ 赞助支持

如果觉得好用，欢迎赞助支持开发者继续维护和开发！

#### 微信赞赏
<img src="docs/wechat_donate.png" width="200" alt="微信赞赏码" />

#### 链动小铺
<img src="docs/ldxp_qrcode.png" width="200" alt="链动小铺二维码" />

🔗 [链动小铺链接](https://pay.ldxp.cn/item/gwd2qo)

#### PayPal
🔗 [paypal.me/koboling](https://paypal.me/koboling)

---

## English

**KoGames** is an open-source game library manager designed for big screen / gamepad mode. It supports auto-importing games from Steam, Epic, EA, Ubisoft, Xbox, GOG and provides a unified launch & management experience.

### ✨ Features

- 🖥️ **Dual Mode UI** — Fullscreen Big Screen + Traditional Desktop mode, one-click switch
- 🎮 **7 Platform Import** — Steam / Epic / EA / Ubisoft / Xbox / GOG / Battle.net
- 📊 **Deep Statistics** — Play time, completion, platform breakdown, Year in Review, milestones
- 🔍 **Smart Search** — Search by name, developer, publisher, tags, genre
- 📌 **Quick Launch** — Pin favorite games to home screen
- 🎬 **Steam Video Preview** — Auto-fetch Steam game videos and covers
- 🖼️ **SteamGridDB Integration** — Auto-fetch covers for non-Steam games
- 🌐 **IGDB Metadata** — Auto-fetch developer, publisher, ratings, descriptions
- 🏷️ **Tag System** — Custom tags with tag-based filtering and grouping
- 🔋 **Battery Status** — Real-time battery display for laptops/handhelds
- 🌙 **Screen Saver** — Auto clock screensaver after 2 minutes of inactivity
- 🌐 **Bilingual** — Auto-detect system language, supports English & Chinese
- 💾 **Backup/Restore** — ZIP backup entire library, auto-backup support
- 🎲 **Random Game Picker** — Can't decide? Pick randomly
- 📝 **Game Notes** — Add personal notes to games
- ⌨️ **Keyboard Shortcuts** — Ctrl+F search, Ctrl+A select all, F11 toggle mode
- 📁 **Import/Export** — JSON format library import/export
- 🎯 **Pre/Post Scripts** — Auto-run scripts before/after game launch
- 🏆 **Achievement System** — Auto-unlock gaming milestones
- 📅 **Year in Review** — Steam Replay-like annual statistics

### 🚀 Getting Started

#### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/)

#### Development

```bash
# Install dependencies
npm install

# Start hot-reload dev
dev.bat
# Or manually
npm run tauri:dev
```

#### Build Installer

```bash
# Build exe installer
build.bat
# Installer will be output to release/ folder
```

### 📦 Tech Stack

- **Frontend**: Svelte 5 + TypeScript + Vite
- **Backend**: Rust + Tauri 2
- **UI**: Custom big-screen console-style interface

### ⭐ Like it?

If KoGames is useful to you, please give it a **Star** ⭐!

[👉 Star this repo](https://github.com/kobolingfeng/KoGames)

### ❤️ Sponsor

If you find it useful, consider supporting the developer!

#### WeChat Donate
<img src="docs/wechat_donate.png" width="200" alt="WeChat Donate" />

#### LDXP Shop
<img src="docs/ldxp_qrcode.png" width="200" alt="LDXP QR Code" />

🔗 [LDXP Shop Link](https://pay.ldxp.cn/item/gwd2qo)

#### PayPal
🔗 [paypal.me/koboling](https://paypal.me/koboling)

---

## 📄 License

[MIT](LICENSE)
