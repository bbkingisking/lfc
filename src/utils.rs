use crate::models::Summary;
use regex::Regex;

pub fn format_summary_plain_text(summary: &Summary) -> String {
    let mut output = String::new();

    // Mood sentence
    output.push_str(&format!("{}\n\n", summary.mood));

    // Bullet points
    for bullet in &summary.items {
        if bullet.accepted == Some(true) {
            output.push_str(&format!("- {}\n\n", bullet.text));
        }
    }

    output.trim().to_string()
}

/// Remove HTML tags from text content to clean up stray tags that make it through scraping
/// 
/// This function is specifically designed to clean up HTML content that sometimes appears
/// in scraped text from This Is Anfield articles. It handles:
/// 
/// - Complete HTML tags (e.g., `<img>`, `<div>`, `<p>`, etc.)
/// - Self-closing tags (e.g., `<br/>`, `<img ... />`)
/// - Malformed/incomplete HTML tags that might appear at text boundaries
/// - WordPress shortcodes (e.g., `[caption]`, `[/caption]`)
/// - HTML entities (both named like `&nbsp;` and numeric like `&#8217;`)
/// - Excessive whitespace and line breaks that result from tag removal
/// 
/// # Arguments
/// 
/// * `text` - The input text that may contain HTML tags and entities
/// 
/// # Returns
/// 
/// A cleaned string with HTML tags removed and entities decoded
/// 
/// # Example
/// 
/// ```
/// use lfc::utils::clean_html_tags;
/// 
/// let dirty = r#"<img src="test.jpg" />Liverpool&rsquo;s victory was <strong>impressive</strong>."#;
/// let clean = clean_html_tags(dirty);
/// assert_eq!(clean, "Liverpool's victory was impressive.");
/// ```
pub fn clean_html_tags(text: &str) -> String {
    // Handle malformed or incomplete HTML tags first
    let incomplete_tag_regex = Regex::new(r"<[^>]*$").unwrap();
    let cleaned = incomplete_tag_regex.replace_all(text, "");
    
    // Create a regex to match HTML tags (including self-closing tags and attributes)
    let html_tag_regex = Regex::new(r"</?[^>]*>").unwrap();
    let cleaned = html_tag_regex.replace_all(&cleaned, "");
    
    // Remove common WordPress/CMS artifacts that might slip through
    let wordpress_regex = Regex::new(r"\[/?[^\]]*\]").unwrap(); // [caption], [/caption], etc.
    let cleaned = wordpress_regex.replace_all(&cleaned, "");
    
    // Clean up common HTML entities
    let cleaned = cleaned.replace("&nbsp;", " ");
    let cleaned = cleaned.replace("&amp;", "&");
    let cleaned = cleaned.replace("&lt;", "<");
    let cleaned = cleaned.replace("&gt;", ">");
    let cleaned = cleaned.replace("&quot;", "\"");
    let cleaned = cleaned.replace("&#039;", "'");
    let cleaned = cleaned.replace("&apos;", "'");
    let cleaned = cleaned.replace("&mdash;", "—");
    let cleaned = cleaned.replace("&ndash;", "–");
    let cleaned = cleaned.replace("&hellip;", "…");
    let cleaned = cleaned.replace("&copy;", "©");
    
    // Handle numeric HTML entities (e.g., &#8217; for apostrophe)
    let numeric_entity_regex = Regex::new(r"&#\d+;").unwrap();
    let cleaned = numeric_entity_regex.replace_all(&cleaned, "'");
    
    // Clean up excessive whitespace that might result from tag removal
    let whitespace_regex = Regex::new(r"\s+").unwrap();
    let cleaned = whitespace_regex.replace_all(&cleaned, " ");
    
    // Remove multiple line breaks
    let linebreak_regex = Regex::new(r"\n\s*\n\s*\n+").unwrap();
    let cleaned = linebreak_regex.replace_all(&cleaned, "\n\n");
    
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_html_tags() {
        // Test basic HTML tag removal
        let input = r#"<p>This is a paragraph</p> with <strong>bold text</strong>."#;
        let expected = "This is a paragraph with bold text.";
        assert_eq!(clean_html_tags(input), expected);

        // Test complex HTML like the example from thisisanfield
        let complex_input = r#"<img loading="lazy" loading="lazy" decoding="async" src="https://www.thisisanfield.com/wp-content/uploads/P2025-09-16-Liverpool_Atletico_MD-1-3-600x400.jpg" alt="LIVERPOOL, ENGLAND - Tuesday, September 16, 2025" width="600" height="400" class="alignnone size-medium wp-image-327377" srcset="https://example.com/image.jpg 600w" sizes="(max-width: 600px) 100vw, 600px" />This is the actual content."#;
        let expected = "This is the actual content.";
        assert_eq!(clean_html_tags(complex_input), expected);
        
        // Test WordPress shortcodes
        let wordpress_input = "[caption id='123' align='left']Image caption[/caption] Regular text here.";
        let expected = "Regular text here.";
        assert_eq!(clean_html_tags(wordpress_input), expected);
        
        // Test malformed HTML tags
        let malformed_input = "Normal text <img src='test' and then incomplete tag <div";
        let expected = "Normal text and then incomplete tag";
        assert_eq!(clean_html_tags(malformed_input), expected);

        // Test HTML entities including numeric ones
        let entity_input = "Liverpool&nbsp;&amp;&nbsp;Atletico &lt;match&gt; &quot;preview&quot; &#039;analysis&#039; &#8217;test&#8217; &mdash; dash";
        let expected = "Liverpool & Atletico <match> \"preview\" 'analysis' 'test' — dash";
        assert_eq!(clean_html_tags(entity_input), expected);

        // Test excessive whitespace cleanup
        let whitespace_input = "<p>Text</p>    <div>More   text</div>   \n\n  <span>End</span>";
        let expected = "Text More text End";
        assert_eq!(clean_html_tags(whitespace_input), expected);

        // Test empty tags
        let empty_tags = "<p></p><div></div>Some content<br/><hr>";
        let expected = "Some content";
        assert_eq!(clean_html_tags(empty_tags), expected);
    }
}
