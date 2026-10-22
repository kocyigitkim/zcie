//! Hook scripts embedded in the binary and installed to `<home>/hooks/`.

use std::path::{Path, PathBuf};

/// Each script: file name and content.
const SCRIPTS: [(&str, &str); 17] = [
    ("common.sh", include_str!("../../../../../adapters/claude-code/hooks/common.sh")),
    ("session-start.sh", include_str!("../../../../../adapters/claude-code/hooks/session-start.sh")),
    ("session-context.sh", include_str!("../../../../../adapters/claude-code/hooks/session-context.sh")),
    ("user-prompt-submit.sh", include_str!("../../../../../adapters/claude-code/hooks/user-prompt-submit.sh")),
    ("inject-context.sh", include_str!("../../../../../adapters/claude-code/hooks/inject-context.sh")),
    ("session-guard.sh", include_str!("../../../../../adapters/claude-code/hooks/session-guard.sh")),
    ("read-guard.sh", include_str!("../../../../../adapters/claude-code/hooks/read-guard.sh")),
    ("post-tool-use.sh", include_str!("../../../../../adapters/claude-code/hooks/post-tool-use.sh")),
    ("stop.sh", include_str!("../../../../../adapters/claude-code/hooks/stop.sh")),
    ("assistant-message.sh", include_str!("../../../../../adapters/claude-code/hooks/assistant-message.sh")),
    ("session-map.sh", include_str!("../../../../../adapters/claude-code/hooks/session-map.sh")),
    ("tool-unlock.sh", include_str!("../../../../../adapters/claude-code/hooks/tool-unlock.sh")),
    ("tool-relock.sh", include_str!("../../../../../adapters/claude-code/hooks/tool-relock.sh")),
    ("verify-gate.sh", include_str!("../../../../../adapters/claude-code/hooks/verify-gate.sh")),
    ("subagent-context.sh", include_str!("../../../../../adapters/claude-code/hooks/subagent-context.sh")),
    ("cursor-hook.sh", include_str!("../../../../../adapters/cursor/cursor-hook.sh")),
    ("codex-hook.sh", include_str!("../../../../../adapters/codex/codex-hook.sh")),
];

/// Writes all scripts to `<home>/hooks/` with mode 0755. Returns the folder.
pub fn install(home: &Path) -> anyhow::Result<PathBuf> {
    let folder = home.join("hooks");
    std::fs::create_dir_all(&folder)?;
    for (name, content) in SCRIPTS {
        let path = folder.join(name);
        std::fs::write(&path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(folder)
}

/// The shell that runs the hook scripts: `sh`, or Git Bash on Windows (ADR-027).
#[cfg(unix)]
pub fn shell() -> PathBuf {
    PathBuf::from("sh")
}

/// The shell that runs the hook scripts: `sh`, or Git Bash on Windows (ADR-027). Order:
/// `ZCIE_BASH`, `CLAUDE_CODE_GIT_BASH_PATH` (set for Claude Code), the default Git folder, the
/// `bin` folder next to `git.exe` on `PATH`.
#[cfg(not(unix))]
pub fn shell() -> PathBuf {
    let from_env = ["ZCIE_BASH", "CLAUDE_CODE_GIT_BASH_PATH"].iter().filter_map(std::env::var_os).map(PathBuf::from);
    let default = [r"C:\Program Files\Git\bin\bash.exe".into()];
    let next_to_git = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .filter(|dir| dir.join("git.exe").is_file())
        .filter_map(|dir| dir.parent().map(|git| git.join(r"bin\bash.exe")));
    from_env.chain(default).chain(next_to_git).find(|path| path.is_file()).unwrap_or_else(|| PathBuf::from("bash.exe"))
}

/// The hook command that runs `script` (an installed script path) with `args`. On Windows the
/// clients start hooks without a POSIX shell, so the command names Git Bash.
pub fn command(script: &Path, args: &str) -> String {
    let script = script.display().to_string();
    let base = if cfg!(unix) { script } else { format!("\"{}\" \"{}\"", shell().display(), script.replace('\\', "/")) };
    if args.is_empty() { base } else { format!("{base} {args}") }
}

/// Returns the names of installed scripts that are missing or differ from the scripts in this
/// binary (for example after an update of `zcie`).
pub fn outdated(home: &Path) -> Vec<&'static str> {
    let folder = home.join("hooks");
    SCRIPTS
        .iter()
        .filter(|(name, content)| std::fs::read_to_string(folder.join(name)).map_or(true, |installed| installed != *content))
        .map(|(name, _)| *name)
        .collect()
}

/// One hook registration: Claude Code event, optional tool matcher, script name.
pub struct HookSpec {
    /// Claude Code hook event.
    pub event: &'static str,
    /// Tool matcher (PreToolUse and PostToolUse only).
    pub matcher: Option<&'static str>,
    /// Script file name in `<home>/hooks/`.
    pub script: &'static str,
    /// Time limit of Claude Code for the hook, in seconds.
    pub timeout: u32,
}

/// Time limit of a hook (seconds). The context hook waits up to 4 s for a query and 2 s for a
/// source expansion; the verification gate runs the check of the project (up to 120 s).
fn timeout_of(script: &str) -> u32 {
    match script {
        "verify-gate.sh" => 150,
        "inject-context.sh" | "session-map.sh" | "read-guard.sh" | "subagent-context.sh" => 10,
        _ => 5,
    }
}

/// The hooks of a profile. `capture` adds the memory capture hooks; `lean` adds the
/// automatic unlock and relock of denied tools; `check` adds the verification gate (ADR-020).
pub fn profile(capture: bool, inject: bool, guards: bool, lean: bool, check: bool) -> Vec<HookSpec> {
    let mut out = Vec::new();
    let mut add = |event, matcher, script| out.push(HookSpec { event, matcher, script, timeout: timeout_of(script) });
    if capture {
        add("SessionStart", None, "session-start.sh");
        add("UserPromptSubmit", None, "user-prompt-submit.sh");
        add("PostToolUse", Some("Edit|Write|MultiEdit|NotebookEdit|Bash"), "post-tool-use.sh");
        add("SessionEnd", None, "stop.sh");
        add("Stop", None, "assistant-message.sh");
    }
    if inject {
        add("UserPromptSubmit", None, "inject-context.sh");
        add("SessionStart", Some("startup"), "session-map.sh");
        // A subagent gets no prompt hook: its task gets the capsule (ADR-024).
        add("PreToolUse", Some("Agent|Task"), "subagent-context.sh");
    }
    if guards {
        add("SessionStart", Some("startup"), "session-context.sh");
        add("UserPromptSubmit", None, "session-guard.sh");
        add("PreToolUse", Some("Read"), "read-guard.sh");
    }
    if lean {
        add("UserPromptSubmit", None, "tool-unlock.sh");
        add("SessionEnd", None, "tool-relock.sh");
    }
    if check {
        add("PostToolUse", Some("Edit|Write|MultiEdit|NotebookEdit|Bash"), "verify-gate.sh");
        add("Stop", None, "verify-gate.sh");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::profile;

    #[test]
    fn the_gate_and_the_context_hook_get_long_enough_time_limits() {
        let hooks = profile(true, true, true, true, true);
        let limit = |script: &str| hooks.iter().filter(|h| h.script == script).map(|h| h.timeout).max().unwrap_or(0);
        assert!(limit("verify-gate.sh") >= 130, "the check of the project can take 120 s");
        assert!(limit("inject-context.sh") >= 8, "query 4 s + expansion 2 s");
        assert!(hooks.iter().filter(|h| h.script == "verify-gate.sh").count() == 2, "PostToolUse and Stop");
    }
}
