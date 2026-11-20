/// Extracts semantic error lines and their surrounding context from terminal/log buffers.
pub fn extract_semantic_errors(buffer: &str) -> String {
    let lines: Vec<&str> = buffer.lines().collect();
    if lines.is_empty() {
        return "Terminal output is empty.".to_string();
    }

    let error_indicators = [
        "error:", "error[", "fatal:", "exception:", "traceback (most recent call last):",
        "panic:", "failed:", "failure:", "syntaxerror:", "typeerror:", "valueerror:",
        "cannot find module", "npm err!", "yarn run v", "cargo:error", "aborting due to",
        "exit code: 1", "exit status 1", "compilation failed", "build failed",
        "[error]", "level=error", "undefined symbol"
    ];

    let mut match_indices = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        if error_indicators.iter().any(|&pattern| lower.contains(pattern)) {
            match_indices.push(idx);
        }
    }

    if match_indices.is_empty() {
        return format!(
            "No syntax errors, stack traces, or fatal panics detected in the last {} terminal lines.",
            lines.len()
        );
    }

    // Collect window around error lines (3 lines before first error up to 8 lines after)
    let first_err = match_indices.first().copied().unwrap_or(0);
    let start_idx = first_err.saturating_sub(3);
    let last_err = match_indices.last().copied().unwrap_or(lines.len() - 1);
    let end_idx = std::cmp::min(lines.len(), last_err + 8);

    let extracted: Vec<&str> = lines[start_idx..end_idx].to_vec();
    format!(
        "[SEMANTIC ERROR EXTRACTION ({} matches found)]\n{}",
        match_indices.len(),
        extracted.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_semantic_errors_found() {
        let buf = "line 1\nline 2\nerror: something broke\nline 4";
        let res = extract_semantic_errors(buf);
        assert!(res.contains("SEMANTIC ERROR EXTRACTION"));
        assert!(res.contains("something broke"));
    }

    #[test]
    fn test_extract_semantic_errors_clean() {
        let buf = "all good\ncompleted successfully";
        let res = extract_semantic_errors(buf);
        assert!(res.contains("No syntax errors"));
    }
}
