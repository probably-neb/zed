pub fn is_html_tag(tag: &str) -> bool {
    tag.starts_with("<")
        && tag.ends_with(">")
        && tag[1..tag.len() - 1].chars().all(|c| {
            c.is_alphanumeric() || c == '-' || c == '_' || c == ':' || c == '/' || c == ' '
        })
}

pub struct TagAutoclose {
    state: (),

    pub fn feed(&mut self, chars: &str) -> Result<(), String> {
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_html_autoclose() {

    }
}
