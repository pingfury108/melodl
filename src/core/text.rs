//! 文本清洗工具

/// 去掉 LRC 中的元信息标签行
pub fn clean_lrc(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let t = line.trim();
            !(t.starts_with("[ti:") || t.starts_with("[ar:") || t.starts_with("[al:") || t.starts_with("[by:") || t.starts_with("[kuwo:"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 去除搜索关键词高亮的 <em> 等 HTML 标签
pub fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// 去除首尾空白, 空字符串转为 None
pub fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// 从 URL 的 query 或 fragment 中提取参数 (兼容 music.163.com/#/playlist?id=xxx)
pub fn extract_url_param(url: &str, key: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let mut sources: Vec<String> = vec![parsed.query().unwrap_or("").to_string()];
    if let Some(frag) = parsed.fragment() {
        if let Some(pos) = frag.find('?') {
            sources.push(frag[pos + 1..].to_string());
        }
    }
    for qs in sources {
        for (k, v) in form_urlencoded::parse(qs.as_bytes()) {
            if k == key && !v.is_empty() {
                return Some(v.into_owned());
            }
        }
    }
    None
}

/// URL 路径最后一段作为 ID (去掉 .html/.htm 后缀)
pub fn extract_path_id(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let seg = parsed.path().trim_end_matches('/').rsplit('/').next()?;
    let id = seg.trim_end_matches(".html").trim_end_matches(".htm");
    (!id.is_empty() && id != "/").then(|| id.to_string())
}
