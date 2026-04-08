use crate::models::MessageRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizedMessageKind {
    Standard,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedMessageContent {
    pub text: String,
    pub kind: NormalizedMessageKind,
}

impl NormalizedMessageContent {
    pub fn role_or(self, default_role: MessageRole) -> (String, MessageRole) {
        let role = match self.kind {
            NormalizedMessageKind::Standard => default_role,
            NormalizedMessageKind::System => MessageRole::System,
        };

        (self.text, role)
    }
}

pub fn extract_tag_content(content: &str, tag_name: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag_name);
    let end_tag = format!("</{}>", tag_name);

    if let Some(start) = content.find(&start_tag) {
        if let Some(end) = content.find(&end_tag) {
            let text_start = start + start_tag.len();
            if text_start < end {
                return Some(content[text_start..end].trim().to_string());
            }
        }
    }

    None
}

fn strip_tag_block(content: &str, tag_name: &str) -> String {
    let start_tag = format!("<{}>", tag_name);
    let end_tag = format!("</{}>", tag_name);
    let mut normalized = content.to_string();

    while let Some(start) = normalized.find(&start_tag) {
        let Some(relative_end) = normalized[start..].find(&end_tag) else {
            break;
        };
        let end = start + relative_end + end_tag.len();
        normalized.replace_range(start..end, "");
    }

    normalized
}

fn strip_hidden_transport_sections(content: &str) -> String {
    strip_tag_block(content, "project-file-references")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn normalize_cli_message(content: &str) -> Option<NormalizedMessageContent> {
    let content = strip_hidden_transport_sections(content);

    if content.contains("<task-notification>") {
        return normalize_task_notification(&content);
    }

    if content.contains("<command-name>") {
        let cmd_name = extract_tag_content(&content, "command-name").unwrap_or_default();
        let cmd_args = extract_tag_content(&content, "command-args").unwrap_or_default();

        if !cmd_name.is_empty() {
            let full_cmd = if cmd_args.is_empty() {
                cmd_name
            } else {
                format!("{} {}", cmd_name, cmd_args)
            };

            return Some(NormalizedMessageContent {
                text: full_cmd,
                kind: NormalizedMessageKind::Standard,
            });
        }
    }

    let filter_tags = [
        "<local-command-caveat>",
        "<local-command-stdout>",
        "<system-reminder>",
        "<command-message>",
    ];

    for tag in filter_tags {
        if content.contains(tag) {
            return None;
        }
    }

    Some(NormalizedMessageContent {
        text: content,
        kind: NormalizedMessageKind::Standard,
    })
}

fn normalize_task_notification(content: &str) -> Option<NormalizedMessageContent> {
    let summary = extract_tag_content(content, "summary").filter(|value| !value.is_empty());
    let status = extract_tag_content(content, "status").filter(|value| !value.is_empty());
    let task_id = extract_tag_content(content, "task-id").filter(|value| !value.is_empty());

    let text = if let Some(summary) = summary {
        summary
    } else if let Some(status) = status {
        match task_id {
            Some(task_id) => format!("Background command {} ({})", status, task_id),
            None => format!("Background command {}", status),
        }
    } else {
        "Background command completed".to_string()
    };

    Some(NormalizedMessageContent {
        text,
        kind: NormalizedMessageKind::System,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_notification_is_normalized_to_system_message() {
        let content = concat!(
            "<task-notification>\n",
            "<task-id>bhss8seor</task-id>\n",
            "<status>completed</status>\n",
            "<summary>Background command \"查找 MqProduceCallback 类\" completed (exit code 0)</summary>\n",
            "</task-notification>\n",
            "Read the output file to retrieve the result: /tmp/demo.output"
        );

        let normalized = normalize_cli_message(content).expect("message should be normalized");
        assert_eq!(normalized.kind, NormalizedMessageKind::System);
        assert_eq!(
            normalized.text,
            "Background command \"查找 MqProduceCallback 类\" completed (exit code 0)"
        );
    }

    #[test]
    fn project_file_references_are_hidden_from_history_text() {
        let content = concat!(
            "请帮我看看 @src/components/chat/MessageInput.vue\n\n",
            "<project-file-references>\n",
            "  <project-file path=\"src/components/chat/MessageInput.vue\" type=\"file\" display-name=\"MessageInput.vue\" />\n",
            "</project-file-references>\n"
        );

        let normalized = normalize_cli_message(content).expect("message should be normalized");
        assert_eq!(
            normalized.text,
            "请帮我看看 @src/components/chat/MessageInput.vue"
        );
        assert_eq!(normalized.kind, NormalizedMessageKind::Standard);
    }
}
