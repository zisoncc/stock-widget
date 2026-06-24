# Stock Widget - 股票行情悬浮窗

一个基于 Rust + Win32 API 开发的半透明悬浮股票/ETF行情监视器。

## 功能特性

- **半透明悬浮窗口** - 使用 Win32 分层窗口技术，窗口约 82% 可见度
- **鼠标拖拽** - 按住鼠标左键拖动窗口位置
- **右键菜单** - 右键弹出上下文菜单：
  - 添加股票/ETF（支持美股代码如 AAPL、中股代码如 600519）
  - 删除股票
  - 立即刷新行情
  - 退出程序
- **持久化配置** - 自动保存已添加的股票列表和窗口位置到 JSON 配置文件
- **自动刷新** - 每 30 秒自动获取最新行情
- **中国股市配色** - 红色表示上涨，绿色表示下跌

## 数据源

通过 Yahoo Finance API 获取实时行情数据。

## 编译运行

### 前置条件

1. 安装 [Rust](https://www.rust-lang.org/tools/install)
2. 确保系统已安装 Windows SDK（通常随 Visual Studio Build Tools 安装）

### 构建

`powershell
cargo build --release
`

生成的可执行文件位于 	arget/release/stock-widget.exe

### 运行

`powershell
cargo run
# 或
.\target\release\stock-widget.exe
`

## 配置文件

首次运行时默认监控: AAPL, GOOGL, MSFT, SPY, QQQ

配置文件位于可执行文件同目录下的 stock_widget_config.json，也可手动编辑。

## 快捷键/操作说明

| 操作 | 效果 |
|------|------|
| 鼠标左键按住拖动 | 移动窗口位置 |
| 鼠标右键 | 弹出菜单 |
| 右键菜单 → 添加股票 | 输入股票代码并添加 |
| 右键菜单 → 删除股票 | 输入编号移除股票 |
| 右键菜单 → 立即刷新 | 立即获取最新行情 |
| 右键菜单 → 退出 | 保存位置并退出 |

## 项目结构

`
├── Cargo.toml          # Rust 项目配置
├── src/
│   ├── main.rs         # 入口点 + 消息循环
│   ├── config.rs       # 配置管理（JSON持久化）
│   ├── stock.rs        # 行情获取（Yahoo Finance API）
│   └── window.rs       # 主窗口（Win32 API）
└── README.md
`

## License

MIT
