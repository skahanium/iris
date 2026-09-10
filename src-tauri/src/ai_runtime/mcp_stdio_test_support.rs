//! Shared launch recipe for the deterministic contract MCP stdio fixture.

use std::path::PathBuf;

/// Locate `name` on the current process PATH without spawning a child.
pub(crate) fn executable_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}

fn contract_mcp_fixture(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(file_name)
}

/// Launch recipe for the contract MCP stdio fixture.
///
/// Windows prefers `node` plus the `.mjs` fixture so parallel `cargo test` does
/// not pay PowerShell cold-start. Unix keeps `/bin/sh` plus the `.sh` fixture
/// so rust-only environments stay PATH-independent.
pub(crate) fn contract_mcp_stdio_command(mode: &str, result_count: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        if let Some(node) = executable_on_path("node") {
            return (
                node.to_string_lossy().into_owned(),
                vec![
                    contract_mcp_fixture("agent-capacity-mcp-stdio.mjs")
                        .to_string_lossy()
                        .into_owned(),
                    mode.to_string(),
                    result_count.to_string(),
                ],
            );
        }
        return (
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".to_string(),
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                contract_mcp_fixture("agent-capacity-mcp-stdio.ps1")
                    .to_string_lossy()
                    .into_owned(),
                mode.to_string(),
                result_count.to_string(),
            ],
        );
    }
    (
        "/bin/sh".to_string(),
        vec![
            contract_mcp_fixture("agent-capacity-mcp-stdio.sh")
                .to_string_lossy()
                .into_owned(),
            mode.to_string(),
            result_count.to_string(),
        ],
    )
}

/// JSON transport config consumed by web-evidence provider rows.
pub(crate) fn contract_mcp_stdio_transport_config(
    mode: &str,
    result_count: &str,
) -> serde_json::Value {
    let (command, args) = contract_mcp_stdio_command(mode, result_count);
    serde_json::json!({ "command": command, "args": args })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn contract_mcp_stdio_command_keeps_posix_shell_off_windows() {
        let (command, args) = contract_mcp_stdio_command("search-fetch", "2");
        if cfg!(windows) {
            let command = command.to_ascii_lowercase();
            assert!(
                command.ends_with("node.exe") || command.ends_with("powershell.exe"),
                "{command}"
            );
            if command.ends_with("node.exe") {
                assert!(
                    args[0].ends_with("agent-capacity-mcp-stdio.mjs"),
                    "{}",
                    args[0]
                );
            }
        } else {
            assert_eq!(command, "/bin/sh");
            assert!(
                args[0].ends_with("agent-capacity-mcp-stdio.sh"),
                "{}",
                args[0]
            );
        }
        assert_eq!(args[args.len() - 2], "search-fetch");
        assert_eq!(args[args.len() - 1], "2");
    }

    #[test]
    fn mjs_fixture_answers_initialize_and_search() {
        let Some(node) = executable_on_path("node") else {
            return;
        };
        let fixture = contract_mcp_fixture("agent-capacity-mcp-stdio.mjs");
        let mut child = Command::new(node)
            .arg(&fixture)
            .arg("search-only")
            .arg("1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("node can spawn the MCP fixture");
        let mut stdin = child.stdin.take().expect("stdin");
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{{}}}}"#
        )
        .unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"search","arguments":{{"query":"topic"}}}}}}"#
        )
        .unwrap();
        drop(stdin);
        let output = child.wait_with_output().expect("fixture exits after EOF");
        assert!(
            output.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), 3, "{stdout}");
        let initialize: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(initialize["id"], 1);
        assert_eq!(
            initialize["result"]["serverInfo"]["name"],
            "iris-contract-mcp"
        );
        let tools: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(tools["result"]["tools"][0]["name"], "search");
        let search: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        let text = search["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("https://source.invalid/contract"), "{text}");
        assert!(text.contains("fact-web-48=value-48"), "{text}");
    }
}
