use askama::Result;

pub fn format_size(size: &usize) -> Result<String> {
    let s = *size as f64;
    let human = if s < 1024.0 {
        format!("{:.0} B", s)
    } else if s < 1024.0 * 1024.0 {
        format!("{:.1} KB", s / 1024.0)
    } else if s < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", s / 1024.0 / 1024.0)
    } else {
        format!("{:.1} GB", s / 1024.0 / 1024.0 / 1024.0)
    };
    Ok(human)
}
