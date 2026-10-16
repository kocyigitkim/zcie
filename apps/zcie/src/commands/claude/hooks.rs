//! Hook scripts embedded in the binary and installed to `<home>/hooks/`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Each script: file name and content.
const SCRIPTS: [(&str, &str); 11] = [
    ("common.sh", include_str!("../../../../../adapters/claude-code/hooks/common.sh")),
    ("session-start.sh", include_str!("../../../../../adapters/claude-code/hooks/session-start.sh")),
    ("session-context.sh", include_str!("../../../../../adapters/claude-code/hooks/session-context.sh")),
    ("user-prompt-submit.sh", include_str!("../../../../../adapters/claude-code/hooks/user-prompt-submit.sh")),
    ("inject-context.sh", include_str!("../../../../../adapters/claude-code/hooks/inject-context.sh")),
    ("session-guard.sh", include_str!("../../../../../adapters/claude-code/hooks/session-guard.sh")),
    ("read-guard.sh", include_str!("../../../../../adapters/claude-code/hooks/read-guard.sh")),
    ("post-tool-use.sh", include_str!("../../../../../adapters/claude-code/hooks/post-tool-use.sh")),
    ("stop.sh", include_str!("../../../../../adapters/claude-code/hooks/stop.sh")),
    ("tool-unlock.sh", include_str!("../../../../../adapters/claude-code/hooks/tool-unlock.sh")),
    ("tool-relock.sh", include_str!("../../../../../adapters/claude-code/hooks/tool-relock.sh")),
];

/// Writes all scripts to `<home>/hooks/` with mode 0755. Returns the folder.
pub fn install(home: &Path) -> anyhow::Result<PathBuf> {
    let folder = home.join("hooks");
    std::fs::create_dir_all(&folder)?;
    for (name, content) in SCRIPTS {
        let path = folder.join(name);
        std::fs::write(&path, content)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(folder)
}

/// One hook registration: Claude Code event, optional tool matcher, script name.
pub struct HookSpec {
    /// Claude Code hook event.
    pub event: &'static str,
    /// Tool matcher (PreToolUse and PostToolUse only).
    pub matcher: Option<&'static str>,
    /// Script file name in `<home>/hooks/`.
    pub script: &'static str,
}

/// The hooks of a profile. `capture` adds the memory capture hooks; `lean` adds the
/// automatic unlock and relock of denied tools.
pub fn profile(capture: bool, inject: bool, guards: bool, lean: bool) -> Vec<HookSpec> {
    let mut out = Vec::new();
    let mut add = |event, matcher, script| out.push(HookSpec { event, matcher, script });
    if capture {
        add("SessionStart", None, "session-start.sh");
        add("UserPromptSubmit", None, "user-prompt-submit.sh");
        add("PostToolUse", Some("Edit|Write|MultiEdit|NotebookEdit|Bash"), "post-tool-use.sh");
        add("SessionEnd", None, "stop.sh");
    }
    if inject {
        add("UserPromptSubmit", None, "inject-context.sh");
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
    out
}
