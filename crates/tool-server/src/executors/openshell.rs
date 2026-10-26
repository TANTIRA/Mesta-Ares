//! `exec.*` tools run inside an NVIDIA OpenShell sandbox
//! (<https://github.com/NVIDIA/OpenShell>, Apache-2.0).
//!
//! OpenShell is CLI-driven: `openshell-sandbox`/`openshell-supervisor` are
//! launched by the gateway, and the Rust SDK (`openshell-sdk`, git-only, not
//! on crates.io) is just a gRPC client to that gateway — there is no
//! embeddable crate that sandboxes an arbitrary local subprocess. So we shell
//! out to the `openshell` binary:
//!
//! - `OPENSHELL_SANDBOX` set → `openshell sandbox exec -n <name> --no-tty
//!   --no-login-shell -- <argv>` (fast path; operator pre-creates the sandbox,
//!   e.g. `openshell sandbox create --name ares-bas`).
//! - unset → `openshell sandbox create --no-keep --no-tty -- <argv>`
//!   (ephemeral sandbox per call; slower, deleted after the command exits).
//!
//! Both paths need a reachable gateway (`openshell gateway add …`, see
//! infra/docker-compose.yml `sandbox` profile). Landlock/seccomp are enforced
//! inside the sandbox's Linux environment — on macOS dev hosts that means the
//! Docker or libkrun VM compute driver, not the host kernel.
//!
//! This layer is defense-in-depth, not the gate: `policies/tools.yml`
//! (`args_schema`) is the gate. We still deny by default — only allowlisted
//! commands, no shell metacharacters — and fail closed when the sandbox
//! cannot be reached. `OPENSHELL_DISABLED=1` or a missing binary returns
//! "sandbox unavailable"; non-exec tools are unaffected.
//!
//! Non-zero remote exit codes are NOT tool errors: OpenShell propagates the
//! command's exit code, and `exit_code` is reported in the output so callers
//! keep stdout/stderr forensics.

use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{info, warn};

use super::Tool;

/// Hard ceiling on `args` length (also enforced by the policy schema).
const MAX_ARGS: usize = 8;
/// Kill the sandboxed command after this long.
const EXEC_TIMEOUT: Duration = Duration::from_secs(30);

/// Commands `exec.*` tools may run. `bash` additionally requires `-c` as the
/// first arg; `sshpass`/`python3` cover the BAS local-exec path; `echo` keeps
/// health-check smoke calls trivial.
const ALLOWED_COMMANDS: &[&str] = &["bash", "sshpass", "python3", "echo"];

/// Substrings that must not appear in any argument — the sandbox is a second
/// line of defense, but nothing here should reach a shell that can chain,
/// pipe, or substitute commands.
const FORBIDDEN_SUBSTRINGS: &[&str] = &[";", "&&", "|", "`", "$("];

/// `exec.sandbox` (and `exec.bas.run`, when allowlisted) — runs `{command,
/// args}` inside an OpenShell sandbox.
pub(super) struct OpenShellExec {
    pub name: &'static str,
}

/// Parse and validate `{command, args}` into an argv, or explain the refusal.
fn validated_argv(args: &Value) -> Result<Vec<String>> {
    let command = args
        .get("command")
        .and_then(Value::as_str)
        .context("missing `command`")?;
    anyhow::ensure!(
        ALLOWED_COMMANDS.contains(&command),
        "command `{command}` not in exec allowlist {ALLOWED_COMMANDS:?}"
    );

    let mut argv = vec![command.to_string()];
    if let Some(raw) = args.get("args") {
        let items = raw
            .as_array()
            .context("`args` must be an array of strings")?;
        anyhow::ensure!(items.len() <= MAX_ARGS, "too many args (max {MAX_ARGS})");
        for item in items {
            let s = item.as_str().context("`args` items must be strings")?;
            argv.push(s.to_string());
        }
    }

    if command == "bash" {
        anyhow::ensure!(
            argv.get(1).map(String::as_str) == Some("-c"),
            "`bash` requires `-c` as the first arg"
        );
    }
    for a in &argv[1..] {
        if let Some(bad) = FORBIDDEN_SUBSTRINGS.iter().find(|p| a.contains(*p)) {
            anyhow::bail!("arg contains forbidden shell metachar `{bad}`");
        }
    }
    Ok(argv)
}

/// `openshell` argv wrapping `argv` — either `sandbox exec` into the named
/// sandbox or an ephemeral `sandbox create --no-keep`.
fn openshell_argv(sandbox: Option<&str>, argv: &[String]) -> Vec<String> {
    let mut v = vec!["sandbox".to_string()];
    match sandbox {
        Some(name) => {
            v.extend(["exec", "-n", name, "--no-tty", "--no-login-shell"].map(String::from));
        }
        None => {
            v.extend(["create", "--no-keep", "--no-tty"].map(String::from));
        }
    }
    v.push("--".to_string());
    v.extend(argv.iter().cloned());
    v
}

#[async_trait]
impl Tool for OpenShellExec {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn execute(&self, args: &Value) -> Result<Value> {
        if super::env("OPENSHELL_DISABLED").as_deref() == Some("1") {
            anyhow::bail!("sandbox unavailable: OPENSHELL_DISABLED=1");
        }
        let bin = super::env("OPENSHELL_BIN").unwrap_or_else(|| "openshell".to_string());
        let argv = validated_argv(args)?;
        let full = openshell_argv(super::env("OPENSHELL_SANDBOX").as_deref(), &argv);
        info!(tool = self.name, bin = %bin, argv = ?full, "exec via openshell");

        let mut cmd = Command::new(&bin);
        cmd.args(&full)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!("sandbox unavailable: openshell binary `{bin}` not found");
            }
            Err(e) => return Err(e).context("spawn openshell"),
        };

        // Drain the pipes on tasks so a chatty child can't deadlock on a full
        // pipe buffer while we only wait().
        let mut out_pipe = child.stdout.take().context("openshell stdout pipe")?;
        let mut err_pipe = child.stderr.take().context("openshell stderr pipe")?;
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            out_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            err_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });

        let status = match tokio::time::timeout(EXEC_TIMEOUT, child.wait()).await {
            Ok(res) => res.context("openshell wait")?,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await; // reap
                warn!(tool = self.name, "openshell exec timed out, killed");
                anyhow::bail!("openshell exec timed out after {}s", EXEC_TIMEOUT.as_secs());
            }
        };
        let stdout = stdout_task.await.context("openshell stdout read")??;
        let stderr = stderr_task.await.context("openshell stderr read")??;
        let exit_code = status.code().unwrap_or(-1);
        info!(tool = self.name, exit_code, "openshell exec finished");
        Ok(json!({
            "stdout": String::from_utf8_lossy(&stdout),
            "stderr": String::from_utf8_lossy(&stderr),
            "exit_code": exit_code,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(command: &str, args: &[&str]) -> Value {
        json!({ "command": command, "args": args })
    }

    #[test]
    fn allowlist_rejects_unlisted_command() {
        let err = validated_argv(&v("rm", &["-rf", "/"])).unwrap_err();
        assert!(err.to_string().contains("not in exec allowlist"));
    }

    #[test]
    fn rejects_shell_metachars_in_args() {
        for bad in [
            v("bash", &["-c", "echo hi; rm -rf /"]),
            v("echo", &["a && b"]),
            v("python3", &["-c", "x|y"]),
            v("echo", &["`id`"]),
            v("echo", &["$(id)"]),
        ] {
            assert!(
                validated_argv(&bad).is_err(),
                "accepted metachar args: {bad}"
            );
        }
    }

    #[test]
    fn bash_requires_dash_c() {
        assert!(validated_argv(&v("bash", &["echo hi"])).is_err());
        assert!(validated_argv(&v("bash", &["-c", "echo hi"])).is_ok());
    }

    #[test]
    fn accepts_benign_echo_shape() {
        let argv = validated_argv(&v("echo", &["hi"])).unwrap();
        assert_eq!(argv, ["echo", "hi"]);
    }

    #[test]
    fn rejects_non_string_and_too_many_args() {
        assert!(validated_argv(&json!({"command": "echo", "args": [1]})).is_err());
        let many = vec!["x"; MAX_ARGS + 1];
        assert!(validated_argv(&v("echo", &many)).is_err());
    }

    #[test]
    fn argv_wraps_sandbox_exec() {
        let argv = validated_argv(&v("echo", &["hi"])).unwrap();
        let full = openshell_argv(Some("ares-bas"), &argv);
        assert_eq!(
            full,
            [
                "sandbox",
                "exec",
                "-n",
                "ares-bas",
                "--no-tty",
                "--no-login-shell",
                "--",
                "echo",
                "hi"
            ]
        );
    }

    #[test]
    fn argv_wraps_ephemeral_create() {
        let argv = validated_argv(&v("echo", &["hi"])).unwrap();
        let full = openshell_argv(None, &argv);
        assert_eq!(
            full,
            [
                "sandbox",
                "create",
                "--no-keep",
                "--no-tty",
                "--",
                "echo",
                "hi"
            ]
        );
    }
}
