mod cli;

use std::io::Write;

use clap::Parser;
use cli::args::{Cli, Commands};
use comfy_table::{Cell, Color, Table};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use melodl::core::registry::available_sources;
use melodl::{MusicClient, ProgressEvent, SongInfo};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let cli = Cli::parse();

    if let Some(Commands::Sources) = cli.command {
        println!("可用音乐源:");
        for name in available_sources() {
            println!("  - {}", name);
        }
        return Ok(());
    }

    let Some(keyword) = cli.keyword.clone() else {
        eprintln!("请提供搜索关键词: melodl -k <关键词> (或用 `melodl sources` 查看可用源)");
        std::process::exit(1);
    };

    let source_names: Vec<String> = cli
        .sources
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let client = MusicClient::builder()
        .sources(source_names.clone())
        .work_dir(cli.work_dir.clone())
        .search_size_per_source(cli.limit)
        .build()?;

    // 搜索(带 spinner)
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner} {msg}").unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));
    spinner.set_message(format!("正在从 {} 搜索「{}」...", source_names.join(","), keyword));

    let songs = client.search(&keyword).await;
    spinner.finish_and_clear();

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
        print!("输入要下载的序号(逗号分隔, 回车取消): ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            println!("已取消");
            return Ok(());
        }
        parse_ids(line, &songs)
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
