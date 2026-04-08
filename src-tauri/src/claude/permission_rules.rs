use serde_json::Value;

pub fn build_allow_rule(tool_name: &str, input: &Value) -> String {
    if tool_name == "Bash" {
        if let Some(command) = bash_command(input) {
            return format!("Bash({command})");
        }
    }

    tool_name.to_string()
}

pub fn matches_allow_rule(rule: &str, tool_name: &str, input: &Value) -> bool {
    if tool_name != "Bash" {
        return rule == tool_name;
    }

    if rule == "Bash" {
        return true;
    }

    let Some(command) = bash_command(input) else {
        return false;
    };

    let Some(inner) = rule
        .strip_prefix("Bash(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return false;
    };

    if let Some(prefix) = inner.strip_suffix(":*") {
        return command.starts_with(prefix);
    }

    command == inner
}

fn bash_command(input: &Value) -> Option<&str> {
    input.get("command").and_then(Value::as_str)
}
