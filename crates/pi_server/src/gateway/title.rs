//! 会话标题生成（自动命名启发式）。
//!
//! 做什么：`extract_message_text` 从 user 消息提取纯文本（候选捕获）；
//! `generate_session_title` 按 Picot 风格启发式从候选生成标题（跳过问候语/
//! 开场白，截取首句、60 字符截断、首字母大写）；`maybe_generate` 封装触发
//! 条件（未命名且 ≥2 轮且有候选）并返回可落库的标题。
//! 不做什么：不维护字段/状态（turn_count/title_set/title_candidates 留在
//! ManagedSession）；不写 DB、不推送（调用方 session_manager 负责）。
//! 依赖：regex（惰性编译）；serde_json（提取 user 消息文本）。

use serde_json::Value;

/// Extract plain text from a user message (handles string or content blocks).
pub(super) fn extract_message_text(msg: &Value) -> String {
    let content = match msg.get("content") {
        Some(c) => c,
        None => return String::new(),
    };
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        return arr
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

/// Generate a session title from captured user messages (Picot-style heuristic).
fn generate_session_title(messages: &[String]) -> Option<String> {
    static GREETING_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static OPENER_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

    let greeting = GREETING_RE.get_or_init(|| {
        regex::Regex::new(r"^(hey|hello|hi|morning|good morning|howdy|yo|sup)[\s!.:,]*$").unwrap()
    });
    let opener = OPENER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)^(ok|okay|so|actually|hey|please|can you|could you|i want(?:ed)? to|i wanna|let'?s)\s+").unwrap()
    });

    // Find first substantive message
    let text = messages
        .iter()
        .find(|m| {
            let trimmed = m.trim();
            !trimmed.is_empty()
                && !greeting.is_match(trimmed)
                && !trimmed.to_lowercase().starts_with("read your memory")
                && !trimmed.to_lowercase().starts_with("read your seed")
                && trimmed.len() >= 10
        })
        .or_else(|| messages.first())?;

    // Strip conversational openers
    let cleaned = opener.replace(text.trim(), "").to_string();
    let first_line = cleaned.lines().next().unwrap_or(&cleaned);

    // Extract first sentence (boundary between char 10-80)
    let char_count = first_line.chars().count();
    let start = 10.min(char_count);
    let title = if let Some(pos) = first_line.chars().skip(start)
        .position(|c| c == '.' || c == '!' || c == '?')
    {
        let end = start + pos + 1;
        first_line.chars().take(end.min(char_count)).collect::<String>()
    } else {
        first_line.to_string()
    };

    // Truncate at 60 chars
    let title = if title.chars().count() > 60 {
        let truncated: String = title.chars().take(57).collect();
        let cut = truncated.rfind(' ').unwrap_or(truncated.len());
        format!("{}…", &truncated[..cut])
    } else {
        title
    };

    // Capitalize first letter
    let mut chars = title.chars();
    let first = chars.next()?;
    let capitalized: String = first.to_uppercase().collect::<String>() + chars.as_str();

    if capitalized.is_empty() {
        None
    } else {
        Some(capitalized)
    }
}

/// 自动命名触发判断 + 生成：未命名且 ≥2 轮且有候选时才产出标题。
/// 调用方负责把标题写回 session（session_name/title_set/dirty/pending_names）。
pub(super) fn maybe_generate(
    instance_id: &str,
    turn_count: u32,
    title_set: bool,
    candidates: &[String],
) -> Option<String> {
    if title_set || turn_count < 2 || candidates.is_empty() {
        return None;
    }
    let title = generate_session_title(candidates)?;
    log::info!("[session_manager] auto-title for {}: {}", instance_id, title);
    Some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_text_from_string_and_blocks() {
        assert_eq!(extract_message_text(&serde_json::json!({"content": "hi"})), "hi");
        assert_eq!(
            extract_message_text(&serde_json::json!({
                "content": [
                    {"type": "text", "text": "a"},
                    {"type": "text", "text": "b"},
                    {"type": "toolCall", "id": "t1"}
                ]
            })),
            "a\nb"
        );
        assert_eq!(extract_message_text(&serde_json::json!({})), "");
    }

    #[test]
    fn skips_greetings_and_falls_back_to_first() {
        // 全是问候语：无实质性消息 → 回退首条（仍产出标题）
        assert_eq!(
            generate_session_title(&["hi".to_string(), "hello!".to_string()]),
            Some("Hi".to_string())
        );
    }

    #[test]
    fn strips_opener_and_capitalizes() {
        assert_eq!(
            generate_session_title(&["please write a function to add two numbers".to_string()]),
            Some("Write a function to add two numbers".to_string())
        );
    }

    #[test]
    fn extracts_first_sentence() {
        assert_eq!(
            generate_session_title(&["explain routing.".to_string()]),
            Some("Explain routing.".to_string())
        );
    }

    #[test]
    fn truncates_long_titles_with_ellipsis() {
        let t = generate_session_title(&["a".repeat(120)]).unwrap();
        assert!(t.chars().count() <= 58, "title too long: {}", t.chars().count());
        assert!(t.ends_with('…'));
    }

    #[test]
    fn maybe_generate_gates_on_state() {
        let candidates = vec!["explain routing.".to_string()];
        // 未满 2 轮 / 已命名 / 无候选 → 不产出
        assert_eq!(maybe_generate("i1", 1, false, &candidates), None);
        assert_eq!(maybe_generate("i1", 2, true, &candidates), None);
        assert_eq!(maybe_generate("i1", 2, false, &[]), None);
        // 满足条件 → 产出标题
        assert_eq!(
            maybe_generate("i1", 2, false, &candidates),
            Some("Explain routing.".to_string())
        );
    }
}
