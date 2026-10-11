mod cli;

use clap::Parser;
use cli::args::{Cli, Commands};
use cli::config::{expand_home, Config};
use comfy_table::{Cell, Color, Table};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::{MultiSelect, Text};
use melodl::core::registry::available_sources;
use melodl::{MusicClient, ProgressEvent, SongInfo};

const DEFAULT_SOURCES: &str = "migu,netease,qq,kuwo,fivesing,kugou";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();
    let config = Config::load();

    match cli.command {
        Some(Commands::Sources) => {
            println!("可用音乐源:");
            for name in available_sources() {
                println!("  - {}", name);
            }
            return Ok(());
        }
        Some(Commands::Config) => {
            match Config::path() {
                Some(p) => {
                    println!("配置文件: {}", p.display());
                    if !p.exists() {
                        println!("(尚未创建, 示例:)");
                        println!(
                            "  sources = [\"migu\", \"netease\", \"qq\", \"kuwo\"]\n  work_dir = \"~/Music/melodl\"\n  limit = 10\n  concurrency = 5"
                        );
                    }
                }
                None => println!("无法确定配置目录"),
            }
            return Ok(());
        }
        None => {}
    }

    // 合并: CLI 参数 > 配置文件 > 内置默认
    let source_names: Vec<String> = cli
        .sources
        .as_deref()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .or(config.sources)
        .unwrap_or_else(|| DEFAULT_SOURCES.split(',').map(str::to_string).collect());
    let work_dir = cli
        .work_dir
        .or_else(|| config.work_dir.as_deref().map(expand_home))
        .unwrap_or_else(|| "./music".into());
    let limit = cli.limit.or(config.limit).unwrap_or(10);
    let concurrency = cli.concurrency.or(config.concurrency).unwrap_or(5);

    // 关键词: 参数缺失时交互输入
    let keyword = match cli.keyword.clone() {
        Some(k) => k,
        None => {
            let k = Text::new("输入搜索关键词:").prompt()?;
            if k.trim().is_empty() {
                println!("已取消");
                return Ok(());
            }
            k.trim().to_string()
        }
    };

    let client = MusicClient::builder()
        .sources(source_names.clone())
        .work_dir(work_dir)
        .search_size_per_source(limit)
        .concurrency(concurrency)
        .build()?;

    // 搜索(带 spinner)
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner} {msg}").unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));
    spinner.set_message(format!("正在从 {} 搜索「{}」...", source_names.join(","), keyword));

    // 错误事件输出到 stderr
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressEvent>(64);
    let err_printer = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            if let ProgressEvent::Error { source, message } = ev {
                eprintln!("[{}] 错误: {}", source, message);
            }
        }
    });

    let songs = client.search_with_events(&keyword, tx).await;
    spinner.finish_and_clear();
    let _ = err_printer.await;

    if songs.is_empty() {
        println!("未找到相关歌曲");
        return Ok(());
    }

    // JSON 模式: 输出后退出
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&songs)?);
        return Ok(());
    }

    print_table(&songs);

    // 选择要下载的歌曲
    let selected: Vec<SongInfo> = if cli.first {
        songs.iter().take(1).cloned().collect()
    } else if cli.all {
        songs.clone()
    } else if let Some(ids) = &cli.ids {
        parse_ids(ids, &songs)
    } else {
        interactive_select(&songs)?
    };

    if selected.is_empty() {
        println!("未选择任何歌曲");
        return Ok(());
    }

    // 下载(带进度条)
    download_with_progress(&client, &selected).await;
    Ok(())
}

fn print_table(songs: &[SongInfo]) {
    let mut table = Table::new();
    table.set_header(vec!["ID", "歌手", "歌名", "大小", "时长", "格式", "来源"]);
    for (i, s) in songs.iter().enumerate() {
        let mut ext = Cell::new(s.ext.as_deref().unwrap_or("-"));
        if matches!(s.ext.as_deref(), Some("flac") | Some("wav") | Some("ape")) {
            ext = ext.fg(Color::Green);
        }
        table.add_row(vec![
            Cell::new(i + 1),
            Cell::new(truncate(&s.singers, 12)),
            Cell::new(truncate(&s.song_name, 30)),
            Cell::new(s.file_size_display()),
            Cell::new(s.duration_display()),
            ext,
            Cell::new(&s.source),
        ]);
    }
    println!("{table}");
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_string()
    }
}

fn parse_ids(ids: &str, songs: &[SongInfo]) -> Vec<SongInfo> {
    ids.split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .filter_map(|i| songs.get(i.saturating_sub(1)).cloned())
        .collect()
}

/// 上下键移动, 空格多选, 回车确认
fn interactive_select(songs: &[SongInfo]) -> anyhow::Result<Vec<SongInfo>> {
    let options: Vec<String> = songs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "{}. {} - {} [{}] {} [{}]",
                i + 1,
                truncate(&s.singers, 10),
                truncate(&s.song_name, 24),
                s.ext.as_deref().unwrap_or("-"),
                s.file_size_display(),
                s.source
            )
        })
        .collect();
    let picked = MultiSelect::new("选择要下载的歌曲 (空格多选, 回车确认):", options)
        .with_page_size(15)
        .raw_prompt()?;
    Ok(picked
        .iter()
        .filter_map(|opt| songs.get(opt.index).cloned())
        .collect())
}

async fn download_with_progress(client: &MusicClient, songs: &[SongInfo]) {
    let mp = MultiProgress::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ProgressEvent>(256);

    // 进度事件渲染任务
    let render = tokio::spawn(async move {
        let mut bars: std::collections::HashMap<String, ProgressBar> = Default::default();
        while let Some(ev) = rx.recv().await {
            match ev {
                ProgressEvent::DownloadStarted { id, total } => {
                    let pb = mp.add(ProgressBar::new(total.unwrap_or(0)));
                    pb.set_style(
                        ProgressStyle::with_template(
                            "{msg} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec})",
                        )
                        .unwrap(),
                    );
                    pb.set_message(id.clone());
                    bars.insert(id, pb);
                }
                ProgressEvent::DownloadProgress { id, downloaded, .. } => {
                    if let Some(pb) = bars.get(&id) {
                        pb.set_position(downloaded);
                    }
                }
                ProgressEvent::DownloadFinished { id, path } => {
                    if let Some(pb) = bars.remove(&id) {
                        pb.finish_with_message(format!("✓ {}", path));
                    }
                }
                ProgressEvent::Error { source, message } => {
                    mp.println(format!("[{}] 错误: {}", source, message)).ok();
                }
                _ => {}
            }
        }
    });

    let paths = client.download_with_events(songs, tx).await;
    let _ = render.await;

    println!("\n下载完成 {}/{} 首:", paths.len(), songs.len());
    for p in &paths {
        println!("  {}", p.display());
    }
}
