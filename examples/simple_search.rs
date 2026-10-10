use melodl::MusicClient;

#[tokio::main]
async fn main() -> melodl::Result<()> {
    let client = MusicClient::builder()
        .sources(["fivesing"])
        .work_dir("./music")
        .search_size_per_source(5)
        .build()?;

    let songs = client.search("周杰伦").await;
    for (i, s) in songs.iter().enumerate() {
        println!(
            "{}. {} - {} [{}] {}",
            i + 1,
            s.singers,
            s.song_name,
            s.ext.as_deref().unwrap_or("-"),
            s.file_size_display()
        );
    }

    if let Some(first) = songs.first() {
        let paths = client.download(std::slice::from_ref(first)).await;
        println!("下载完成: {:?}", paths);
    }
    Ok(())
}
