# melodl —— musicdl 的 Rust 重实现方案

> 参考项目: https://github.com/CharlesPikachu/musicdl
> 目标: 用 Rust 实现一个**库优先 (lib-first)** 的音乐搜索/下载工具, 同时产出 CLI 二进制与可被其他程序集成的 crate。
>
> ⚠️ 免责声明: 本项目仅供学习与研究使用。所有内容均来自公开可访问的 Web 接口, 不托管、存储或分发任何版权内容。请尊重各音乐平台的版权与服务条款, 付费/订阅内容请通过官方授权渠道获取。

---

## 一、原版 musicdl 分析结论

### 1.1 项目定位

纯 Python 音乐搜索/下载工具, 支持 **59 个音乐源**, 分 4 类:

| 分类 | 数量 | 例子 | 特点 |
|---|---|---|---|
| `sources/` 官方/独立平台 | 31 | 网易云、QQ、酷狗、酷我、咪咕、Spotify | JSON API 为主, 部分接口有加密/签名 |
| `audiobooks/` 有声/电台 | 5 | 喜马拉雅、蜻蜓FM、荔枝 | 有 episodes(多集) 概念 |
| `common/` 聚合网关 | 6 | GD音乐台、煎饼搜 | 一个客户端聚合多个上游源 |
| `thirdpartysites/` 第三方站 | 17 | 歌曲宝、下歌吧 | HTML 爬虫, 脆弱易失效 |

### 1.2 核心架构(需要移植的精华)

```
musicdl.py (MusicClient 门面)
    └── modules/sources/base.py (BaseMusicClient 基类)
            ├── search(keyword)
            │     ├── _constructsearchurls(keyword, rule) → [url]   # 子类实现: 分页拼 URL
            │     └── _search(keyword, url) → [SongInfo]            # 子类实现: 解析单页
            ├── download(song_infos)
            │     └── _download(song_info)   # 基类通用: HTTP 直链 / HLS(m3u8)
            └── parseplaylist(url)           # 子类可选实现
```

关键组件:

- **`SongInfo`**: 核心数据模型 — `source / song_name / singers / album / ext / file_size_bytes / duration / bitrate / lyric / cover_url / download_url / protocol(HTTP|HLS) / identifier / episodes / raw_data`
- **`AudioLinkTester`**: HEAD 请求验证下载链接 → 失败回退 GET 采样 8KB → 从 Content-Type / URL 后缀 / 文件魔数推断音频格式与大小
- **`SongInfoUtils`**: 下载后处理 — 保存 `.lrc` 歌词、写入音频标签(标题/歌手/专辑/封面)
- **加密工具**: 每个平台一套(网易云 weapi=AES-CBC+RSA / eapi=AES-ECB, QQ sign, 咪咕自定义字节解密, 酷狗/酷我签名)
- **CLI**: click + rich 表格 + prompt_toolkit 光标多选 + 线程池并行

### 1.3 技术映射(Python → Rust)

| Python | Rust |
|---|---|
| requests + ThreadPoolExecutor | `reqwest` + `tokio` 异步并发 |
| click | `clap` |
| rich 进度条/表格 | `indicatif` + `comfy-table` |
| prompt_toolkit 光标选择 | `inquire::MultiSelect` |
| beautifulsoup4 | `scraper` |
| pycryptodome (AES/RSA) | `aes` / `cbc` / `ecb` + `rsa` |
| mutagen (标签/封面) | `lofty` |
| m3u8 + ffmpeg (HLS) | `m3u8-rs` + 系统 ffmpeg(可选) |
| json_repair | `serde_json` + 容错预处理 |
| fake_useragent | 静态 UA 池 |
| logging | `tracing` + `tracing-subscriber` |

---

## 二、总体设计: lib-first

### 2.1 核心原则

**所有能力都在库 (`melodl` lib) 上, CLI (`melodl` bin) 只是薄壳**: 参数解析 + 终端交互 + 调用 lib。这样一个 crate 同时产出:

- `melodl` 库 — 其他 Rust 程序 `melodl = "0.1"` 集成
- `melodl` 二进制 — `cargo install melodl` 命令行使用

### 2.2 目录结构

```
melodl/
├── Cargo.toml
├── plan.md
├── README.md
├── src/
│   ├── lib.rs               # 公共 API 导出
│   ├── main.rs              # CLI 入口(薄壳, feature = "cli")
│   ├── client.rs            # MusicClient 门面: 多源并发 search/download
│   ├── error.rs             # 统一错误类型 (thiserror)
│   ├── core/
│   │   ├── mod.rs
│   │   ├── song.rs          # SongInfo (serde, 全字段 Serialize)
│   │   ├── source.rs        # MusicSource trait + SearchCtx
│   │   ├── registry.rs      # 源注册表: 名称 → 构造函数
│   │   ├── tester.rs        # AudioLinkTester 移植
│   │   ├── downloader.rs    # HTTP 流式下载 + HLS
│   │   ├── tags.rs          # 歌词保存 + lofty 写标签/封面
│   │   └── progress.rs      # ProgressEvent 事件定义
│   ├── crypto/              # 平台加密模块
│   │   ├── mod.rs
│   │   ├── netease.rs       # weapi (AES-CBC+RSA) / eapi (AES-ECB)
│   │   ├── migu.rs          # 自定义字节解密
│   │   ├── kugou.rs
│   │   ├── kuwo.rs
│   │   └── qq.rs            # sign
│   ├── sources/             # 每个平台一个文件, 与原版一一对应
│   │   ├── mod.rs
│   │   ├── migu.rs
│   │   ├── fivesing.rs
│   │   ├── kuwo.rs
│   │   ├── netease.rs
│   │   ├── qq.rs
│   │   ├── kugou.rs
│   │   └── ...
│   └── cli/                 # feature = "cli", 仅 bin 使用
│       ├── mod.rs
│       ├── args.rs          # clap 参数定义
│       └── interactive.rs   # inquire 多选 + indicatif 进度渲染
└── examples/
    └── simple_search.rs     # 库集成示例
```

### 2.3 核心 trait 设计

```rust
#[async_trait]
pub trait MusicSource: Send + Sync {
    fn name(&self) -> &'static str;
    async fn search(&self, keyword: &str, ctx: &SearchCtx) -> Result<Vec<SongInfo>>;
    /// 可选: 解析歌单
    async fn parse_playlist(&self, _url: &str, _ctx: &SearchCtx) -> Result<Vec<SongInfo>> {
        Ok(vec![])
    }
    // download 由通用 Downloader 处理, 源无需实现
}
```

### 2.4 库的公共 API

```rust
use melodl::{MusicClient, ProgressEvent};

let client = MusicClient::builder()
    .sources(["migu", "netease", "kuwo"])
    .work_dir("./music")
    .concurrency_per_source(5)
    .build()?;

// 搜索(所有源并发)
let results = client.search("周杰伦").await?;

// 下载
let downloaded = client.download(&results[..3]).await?;

// 带进度事件(供 CLI/GUI 渲染)
let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressEvent>(100);
client.search_with_events("周杰伦", tx).await?;
```

### 2.5 进度事件(库不直接打印)

```rust
pub enum ProgressEvent {
    SearchStarted   { source: &'static str },
    SearchFinished  { source: &'static str, count: usize },
    DownloadProgress { id: String, downloaded: u64, total: Option<u64> },
    DownloadFinished { id: String, path: PathBuf },
    Error { source: &'static str, message: String },
}
```

### 2.6 CLI 特性门控

`clap` / `indicatif` / `inquire` 只被 bin 使用, 作为库引入时保持依赖干净:

```toml
[features]
default = []
cli = ["dep:clap", "dep:indicatif", "dep:inquire", "dep:comfy-table"]

[lib]
name = "melodl"

[[bin]]
name = "melodl"
required-features = ["cli"]
```

### 2.7 CLI 用法(对齐原版)

```bash
melodl                                   # 交互模式: 搜索 → 多选 → 下载
melodl -k "周杰伦"                        # 关键词搜索 + 交互选择
melodl -k "周杰伦" --first               # 非交互: 直接下第一个结果
melodl -k "周杰伦" --json                # JSON 输出(供其他语言程序集成)
melodl -p "https://music.163.com/playlist?id=xxx"   # 歌单解析下载
melodl -k "周杰伦" -m migu,netease,kuwo  # 指定源
melodl sources                           # 列出所有可用源
```

---

## 三、依赖选型(Cargo.toml 草案)

```toml
[dependencies]
# 异步 & HTTP
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "stream", "cookies", "gzip", "brotli"] }
async-trait = "0.1"
futures = "0.3"

# 数据
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"

# HTML 解析(第三方站)
scraper = "0.23"

# 加密
aes = "0.8"
cbc = "0.1"
ecb = "0.1"
rsa = "0.9"
md5 = "0.7"
sha1 = "1"
sha2 = "0.10"
hmac = "0.12"
base64 = "0.22"
hex = "0.4"

# 音频
lofty = "0.22"          # 标签/封面
m3u8-rs = "6"           # HLS(可选 feature)

# 工具
tracing = "0.1"
tracing-subscriber = "0.3"
sanitize-filename = "0.6"
rand = "0.9"

# CLI(feature = "cli")
clap = { version = "4", features = ["derive"], optional = true }
indicatif = { version = "0.17", optional = true }
inquire = { version = "0.7", optional = true }
comfy-table = { version = "7", optional = true }
```

---

## 四、分阶段实施

### P0 — 骨架(跑通主链路)

- [ ] `cargo init` 工程, lib + bin 结构
- [ ] `SongInfo` / 错误类型 / `MusicSource` trait / registry
- [ ] `AudioLinkTester` 移植(HEAD → GET 采样, ext 推断)
- [ ] HTTP 流式下载器(分块写盘 + ProgressEvent)
- [ ] clap CLI: `-k` 搜索 → 打印表格 → `--first/--all` 下载
- [ ] **验收**: `melodl -k "xxx" --first -m <源>` 能下载一首 mp3

### P1 — 首批源(3 个, 简单优先)

| 源 | 原因 |
|---|---|
| **5sing** | 纯 JSON API, 无加密, 最简 |
| **咪咕** | JSON API + 简单字节解密, 验证 crypto 模块 |
| **酷我** | JSON API + 轻签名, 国内主力源 |

- [ ] `sources/fivesing.rs` / `sources/migu.rs` / `sources/kuwo.rs`
- [ ] `crypto/migu.rs` / `crypto/kuwo.rs`
- [ ] **验收**: 三个源并发搜索, 去重, 展示, 下载成功

### P2 — 主力源(加密重头) ✅ 已完成

- [x] **网易云**: eapi (AES-ECB) 已实现; 匿名 cookie 已失效, 实际走 kangqiovo/bugpk 第三方兜底
- [x] **QQ**: 搜索改走 shc.y.qq.com/soso 老接口 (u.y.qq.com 搜索有 TLS 指纹风控); vkey 下载正常
- [x] **酷狗**: 老移动接口 getSongInfo (免费歌直链) + baka.plus 兜底; gateway 签名已实现但需设备验证未启用
- [x] 歌词获取与 `.lrc` 保存 (含酷我逐字歌词转换)
- [ ] 音频标签/封面写入 (lofty) — 移至 P3

> P2 关键发现: ① QQ 搜索端点做 JA3 指纹风控, Python/OpenSSL 可过, curl/rustls 被拦; ② 环境代理会干扰部分第三方 API, 默认客户端已禁用环境代理 (`no_proxy`); ③ QQ searchid 必须按官方随机公式生成, 否则返回空结果。

### P3 — 体验增强 ✅ 已完成

- [x] 交互式 TUI: `inquire` 关键词输入 + MultiSelect 空格多选 + `indicatif` 多进度条
- [x] 配置文件 `~/.config/melodl/melodl.toml` (默认源/下载目录/数量/并发, CLI 参数覆盖)
- [x] `--json` 输出模式(供外部程序集成)
- [x] 音频标签/封面写入 (lofty, mp3/flac 已验证: TITLE/ARTIST/ALBUM + APIC 封面)
- [x] 下载并发限流 (buffer_unordered)
- [x] **验收**: `melodl config` 查看配置, 裸 `melodl` 进入完整交互流程

### P4 — 扩展(按需渐进)

- [ ] HLS 下载 (m3u8-rs + ffmpeg remux)
- [ ] 歌单解析 `parse_playlist`(网易云/QQ/5sing)
- [ ] 聚合源: gdstudio / tunehub / jbsou
- [ ] 有声源(episodes): 喜马拉雅 / 蜻蜓FM
- [ ] 更多第三方站(scraper)
- [ ] 单元测试 + mock HTTP 集成测试
- [ ] CI: clippy / fmt / release 二进制构建

> 注: 原版 59 个源不全做。第三方爬虫站脆弱易失效, 按需增量添加; 架构保证"加一个源 = 新增一个文件 + registry 注册一行"。

---

## 五、关键设计决策

1. **并发模型**: tokio 异步替代原版线程池。源间并发用 `FuturesUnordered`, 源内分页用 semaphore 限流; 单文件下载分块流式写盘。
2. **源注册**: 静态注册表 `HashMap<&'static str, fn() -> Box<dyn MusicSource>>`, `sources/mod.rs` 集中注册, 编译期确定(不需要运行时插件)。
3. **下载与源解耦**: 源只负责产出 `SongInfo { download_url, protocol, ... }`, 下载统一由 `Downloader` 处理(HTTP 直链 / HLS / 预下载内容)。
4. **去重**: 按 `source + identifier` 去重(对齐原版 `_removeduplicates`)。
5. **文件命名**: `{song_name} - {identifier}.{ext}`, 重名追加 `(n)`, 路径长度截断(对齐原版 `legalizepathlength`)。
6. **错误处理**: 单源失败不影响其他源; 用 `ProgressEvent::Error` 上报, 库层面返回部分成功结果。
7. **配置**: `melodl.toml` (config dirs 标准位置) + CLI 参数覆盖 + 环境变量, 优先级: CLI > env > config > default。
8. **免责声明**: README 与 `--help` 中保留"仅供学习研究"声明。

---

## 六、风险与对策

| 风险 | 对策 |
|---|---|
| 平台 API 加密逻辑复杂(网易云/QQ) | 先 P1 无加密源验证架构; 加密模块独立成 `crypto/`, 逐个击破; 参考原版 utils 逐行移植 |
| 第三方 API 变动失效 | AudioLinkTester 统一验证链接有效性; 单源失败隔离; 错误信息带源名便于定位 |
| HLS/DRM 内容 | P4 阶段处理; DRM 内容不做(原版也依赖外部工具且声明禁止绕过) |
| 大文件下载内存占用 | reqwest stream + tokio::fs 分块写盘, 不缓存整个文件 |

---

## 七、里程碑与交付物

| 里程碑 | 交付物 |
|---|---|
| M1 (P0+P1) | 可搜索下载的 CLI, 3 个源, lib API 可用 |
| M2 (P2) | 6 个国内主力源, 歌词/标签/封面完整 |
| M3 (P3) | 完整交互体验 + 配置文件 + JSON 输出 |
| M4 (P4) | HLS / 歌单 / 更多源, 发布 v0.1.0 |
