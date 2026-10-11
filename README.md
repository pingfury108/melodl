# melodl

musicdl 的 Rust 重实现: 轻量音乐搜索与下载工具, lib-first 设计, 可同时作为 CLI 和库使用。

> ⚠️ 仅供学习与研究使用。请尊重各音乐平台的版权与服务条款。

## CLI 使用

```bash
cargo build --release --features cli

melodl                                  # 交互模式: 输入关键词 → 空格多选 → 下载
melodl sources                          # 列出可用源
melodl config                           # 查看配置文件路径与示例
melodl -k "晴天"                         # 搜索 + 表格展示 + 交互多选下载
melodl -k "晴天" --first                # 直接下载第一个结果
melodl -k "晴天" --all                  # 下载全部
melodl -k "晴天" -i 1,3                 # 下载指定序号
melodl -k "晴天" --json                 # JSON 输出(供其他程序集成)
melodl -k "晴天" -m netease,kuwo -n 20  # 指定源和数量
```

## 配置文件

`~/.config/melodl/melodl.toml` (CLI 参数优先于配置):

```toml
sources = ["migu", "netease", "qq", "kuwo", "fivesing", "kugou"]
work_dir = "~/Music/melodl"
limit = 10
concurrency = 5
```

下载的音频自动写入标签(标题/歌手/专辑/封面)并保存同名 `.lrc` 歌词。

## 作为库集成

```toml
[dependencies]
melodl = { path = "..." }   # 不含 cli feature, 依赖干净
```

```rust
use melodl::MusicClient;

let client = MusicClient::builder()
    .sources(["fivesing"])
    .work_dir("./music")
    .search_size_per_source(10)
    .build()?;

let songs = client.search("周杰伦").await;           // 多源并发搜索
let paths = client.download(&songs[..3]).await;      // 并发下载
```

进度通过 `search_with_events` / `download_with_events` 的 `mpsc::Sender<ProgressEvent>` 上报, 库本身不打印 stdout。

## 远程构建 (rdev)

```bash
rdev cargo build --features cli       # 同步 + 远端构建
rdev sh < scripts/xxx.sh              # 远端执行脚本
```

## 开发计划

见 [plan.md](plan.md)。
