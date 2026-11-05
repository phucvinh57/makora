pub fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn clip(text: &str, max_chars: usize) -> String {
    single_line(text).chars().take(max_chars).collect()
}
