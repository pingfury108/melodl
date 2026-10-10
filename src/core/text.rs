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
