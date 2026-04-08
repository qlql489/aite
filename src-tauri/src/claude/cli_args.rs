use std::collections::BTreeSet;

use tracing::warn;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReservedCliFlag {
    canonical: &'static str,
    alias: Option<&'static str>,
    takes_value: bool,
}

const RESERVED_CLI_FLAGS: &[ReservedCliFlag] = &[
    ReservedCliFlag {
        canonical: "--sdk-url",
        alias: None,
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--print",
        alias: Some("-p"),
        takes_value: false,
    },
    ReservedCliFlag {
        canonical: "--input-format",
        alias: None,
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--output-format",
        alias: None,
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--verbose",
        alias: None,
        takes_value: false,
    },
    ReservedCliFlag {
        canonical: "--include-partial-messages",
        alias: None,
        takes_value: false,
    },
    ReservedCliFlag {
        canonical: "--resume",
        alias: Some("-r"),
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--continue",
        alias: Some("-c"),
        takes_value: false,
    },
    ReservedCliFlag {
        canonical: "--session-id",
        alias: None,
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--model",
        alias: None,
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--effort",
        alias: None,
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--settings",
        alias: None,
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--permission-mode",
        alias: None,
        takes_value: true,
    },
    ReservedCliFlag {
        canonical: "--permission-prompt-tool",
        alias: None,
        takes_value: true,
    },
];

fn split_flag_token(token: &str) -> (&str, bool) {
    if let Some((flag, _)) = token.split_once('=') {
        (flag, true)
    } else {
        (token, false)
    }
}

fn find_reserved_flag(token: &str) -> Option<ReservedCliFlag> {
    let (flag, _) = split_flag_token(token);

    RESERVED_CLI_FLAGS.iter().copied().find(|reserved| {
        reserved.canonical == flag || reserved.alias.is_some_and(|alias| alias == flag)
    })
}

fn collect_conflicting_flags(args: &[String]) -> BTreeSet<&'static str> {
    args.iter()
        .filter_map(|arg| find_reserved_flag(arg).map(|flag| flag.canonical))
        .collect()
}

pub fn reserved_cli_flags_for_display() -> Vec<&'static str> {
    RESERVED_CLI_FLAGS
        .iter()
        .map(|flag| flag.canonical)
        .collect()
}

pub fn validate_custom_cli_args(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg.trim().is_empty()) {
        return Err("自定义 Claude CLI 参数中包含空参数，请检查输入格式".to_string());
    }

    let conflicts = collect_conflicting_flags(args);
    if !conflicts.is_empty() {
        let joined = conflicts.into_iter().collect::<Vec<_>>().join("、");
        return Err(format!(
            "这些参数由应用启动流程接管，不能重复自定义：{}",
            joined
        ));
    }

    Ok(())
}

pub fn sanitize_custom_cli_args(args: &[String]) -> Vec<String> {
    let mut sanitized = Vec::new();
    let mut skipped = BTreeSet::new();
    let mut index = 0;

    while index < args.len() {
        let token = &args[index];

        if let Some(flag) = find_reserved_flag(token) {
            skipped.insert(flag.canonical);

            let (_, has_inline_value) = split_flag_token(token);
            index += 1;

            if flag.takes_value && !has_inline_value && index < args.len() {
                index += 1;
            }

            continue;
        }

        sanitized.push(token.clone());
        index += 1;
    }

    if !skipped.is_empty() {
        warn!(
            "检测到被保留的 Claude CLI 自定义参数，已在启动时忽略: {}",
            skipped.into_iter().collect::<Vec<_>>().join(", ")
        );
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use super::{sanitize_custom_cli_args, validate_custom_cli_args};

    #[test]
    fn rejects_reserved_flags() {
        let args = vec!["--model".to_string(), "sonnet".to_string()];
        let error = validate_custom_cli_args(&args).expect_err("should reject reserved flag");
        assert!(error.contains("--model"));
    }

    #[test]
    fn strips_reserved_flags_with_values() {
        let args = vec![
            "--plugin-dir".to_string(),
            "/tmp/plugins".to_string(),
            "--settings".to_string(),
            "{\"foo\":true}".to_string(),
            "--allowed-tools".to_string(),
            "Read".to_string(),
        ];

        assert_eq!(
            sanitize_custom_cli_args(&args),
            vec![
                "--plugin-dir".to_string(),
                "/tmp/plugins".to_string(),
                "--allowed-tools".to_string(),
                "Read".to_string(),
            ]
        );
    }

    #[test]
    fn strips_inline_reserved_values() {
        let args = vec![
            "--plugin-dir=/tmp/plugins".to_string(),
            "--model=sonnet".to_string(),
        ];

        assert_eq!(
            sanitize_custom_cli_args(&args),
            vec!["--plugin-dir=/tmp/plugins".to_string()]
        );
    }
}
