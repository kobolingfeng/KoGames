# KoGames 🎮

**KoGames** 是一个开源的游戏库管理器，专为大屏/手柄模式设计，支持自动导入 Steam、Epic、EA、Ubisoft、Xbox、GOG 等平台的游戏，并提供统一的启动与管理体验。

![KoGames](https://img.shields.io/badge/KoGames-Game%20Library%20Manager-6366f1?style=for-the-badge)
![License](https://img.shields.io/badge/license-MIT-green?style=for-the-badge)
![Platform](https://img.shields.io/badge/platform-Windows-blue?style=for-the-badge)

## ✨ 功能特色

- 🖥️ **大屏模式** — 全屏沉浸式 UI，支持手柄/键盘操控
- 🎮 **多平台导入** — 自动识别 Steam / Epic / EA / Ubisoft / Xbox / GOG 游戏
- 📊 **游戏统计** — 游戏时长、通关状态、平台分布一目了然
- 🔍 **快速搜索** — 即时搜索所有游戏
- 📌 **快捷启动** — 固定常玩游戏到首页
- 🎬 **Steam 视频预览** — 自动获取 Steam 游戏视频和封面
- 🔋 **电池状态** — 笔记本/掌机电量实时显示
- 🌙 **待机屏保** — 2 分钟无操作自动进入时钟屏保

## 🚀 快速开始

### 前置依赖

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/)

### 开发模式

```bash
# 安装依赖
npm install

# 启动热重载开发
dev.bat
# 或手动运行
npm run tauri:dev
```

### 构建安装包

```bash
# 构建 exe 安装包
build.bat
# 安装包将输出到 release/ 文件夹
```

## 📦 技术栈

- **前端**: Svelte 5 + TypeScript + Vite
- **后端**: Rust + Tauri 2
- **UI**: 自定义大屏主机风格界面

## ⭐ 觉得好用？

如果 KoGames 对你有帮助，请给个 **Star** ⭐ 支持一下！

[👉 点击这里给个 Star](https://github.com/kobolingfengfeng/KoGames)

## ❤️ 赞助支持

如果觉得好用，欢迎赞助支持开发者继续维护和开发！

### 微信赞赏
<img src="docs/wechat_donate.png" width="200" alt="微信赞赏码" />

### 链动小铺
<img src="docs/ldxp_qrcode.png" width="200" alt="链动小铺二维码" />

🔗 [链动小铺链接](https://pay.ldxp.cn/item/gwd2qo)

### PayPal
🔗 [paypal.me/koboling](https://paypal.me/koboling)

## 📄 License

[MIT](LICENSE)
