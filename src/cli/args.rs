use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "melodl", version, about = "音乐搜索下载工具 (仅供学习研究使用)")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// 搜索关键词 (不填则进入交互模式)
    #[arg(short = 'k', long, global = true)]
    pub keyword: Option<String>,

    /// 音乐源, 逗号分隔 (默认见配置文件或内置)
    #[arg(short = 'm', long, global = true)]
    pub sources: Option<String>,

    /// 下载目录
    #[arg(short = 'o', long, global = true)]
    pub work_dir: Option<PathBuf>,

    /// 每源搜索结果数量
    #[arg(short = 'n', long, global = true)]
    pub limit: Option<usize>,

    /// 下载并发数
    #[arg(short = 'j', long, global = true)]
    pub concurrency: Option<usize>,

    /// 直接下载第一个结果(非交互)
    #[arg(long, global = true)]
    pub first: bool,

    /// 下载全部结果(非交互)
    #[arg(long, global = true)]
    pub all: bool,

    /// 按序号下载, 逗号分隔, 如 1,3,5
    #[arg(short = 'i', long, global = true)]
    pub ids: Option<String>,

    /// 歌单 URL, 解析歌单内所有歌曲
    #[arg(short = 'p', long, global = true)]
    pub playlist: Option<String>,

    /// JSON 输出搜索结果(供其他程序集成)
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 列出所有可用音乐源
    Sources,
    /// 打印配置文件路径
    Config,
}
