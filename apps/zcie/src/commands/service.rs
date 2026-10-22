//! `zcie daemon install|uninstall`: start `zcied` at login with a launchd agent (macOS) or a
//! hidden script in the Startup folder of the user (Windows, ADR-027).
//! Without the daemon the hooks do nothing, so the memory gets gaps that nobody sees.
//!
//! The agent starts a copy of `zcied` in `<home>/bin`. The copy keeps working when the npm or nvm
//! folder of the package changes, and it is not in a folder that macOS protects (Desktop,
//! Documents), where launchd cannot open a program.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

/// launchd label of the agent of the default home `~/.zcie`.
pub const LABEL: &str = "dev.zcie.zcied";

/// Returns the launchd label of a home: `LABEL` for `~/.zcie`, else `LABEL.<hash>`, so that a
/// second installation (a test home) does not replace the agent of the first one.
pub fn label(home: &Path) -> String {
    let default = crate::commands::user_home().map(|h| PathBuf::from(h).join(".zcie")).unwrap_or_default();
    if home == default {
        return LABEL.to_string();
    }
    let hash = blake3::hash(home.display().to_string().as_bytes()).to_hex();
    format!("{LABEL}.{}", &hash[..8])
}

/// Returns the path of the launchd agent file (macOS) or the Startup script (Windows) of a home.
pub fn agent_file(home: &Path) -> anyhow::Result<PathBuf> {
    if cfg!(windows) {
        let roaming = std::env::var("APPDATA").context("APPDATA is not set")?;
        return Ok(PathBuf::from(roaming).join(r"Microsoft\Windows\Start Menu\Programs\Startup").join(format!("{}.vbs", label(home))));
    }
    let user_home = crate::commands::user_home().context("HOME is not set")?;
    Ok(PathBuf::from(user_home).join("Library/LaunchAgents").join(format!("{}.plist", label(home))))
}

/// Returns the launchd agent text. `KeepAlive` restarts the daemon after a crash, but not after
/// a clean stop (`zcie daemon stop` exits with code 0).
pub fn agent_text(daemon: &Path, home: &Path) -> String {
    let log = home.join("instances/local/telemetry/logs/zcied.out");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{}</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>--home</string><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        xml(&label(home)),
        xml(&daemon.display().to_string()),
        xml(&home.display().to_string()),
        xml(&log.display().to_string()),
        xml(&log.display().to_string()),
    )
}

/// Returns the Windows Startup script: it starts `zcied` without a window.
/// shortcut: no restart after a crash (launchd has KeepAlive); add a Task Scheduler task if needed.
pub fn startup_script(daemon: &Path, home: &Path) -> String {
    let quote = |path: &Path| path.display().to_string().replace('"', "\"\"");
    format!("CreateObject(\"WScript.Shell\").Run \"\"\"{}\"\" --home \"\"{}\"\"\", 0, False\r\n", quote(daemon), quote(home))
}

/// Starts the Windows Startup script now.
fn run_startup_script(file: &Path) -> anyhow::Result<()> {
    let status = Command::new("wscript.exe").arg(file).status()?;
    if !status.success() {
        bail!("wscript {} failed", file.display());
    }
    Ok(())
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn gui_domain() -> anyhow::Result<String> {
    let out = Command::new("id").arg("-u").output()?;
    Ok(format!("gui/{}", String::from_utf8_lossy(&out.stdout).trim()))
}

/// Copies `zcied` to `<home>/bin/zcied` if the copy is missing or different. Returns the copy
/// and true if it was changed.
pub fn sync_daemon_copy(daemon: &Path, home: &Path) -> anyhow::Result<(PathBuf, bool)> {
    let copy = home.join("bin").join(format!("zcied{}", std::env::consts::EXE_SUFFIX));
    let same = std::fs::read(&copy).ok().zip(std::fs::read(daemon).ok()).is_some_and(|(a, b)| a == b);
    if !same {
        std::fs::create_dir_all(home.join("bin"))?;
        let partial = copy.with_extension("new");
        std::fs::copy(daemon, &partial).with_context(|| format!("cannot copy {}", daemon.display()))?;
        std::fs::rename(&partial, &copy)?;
    }
    Ok((copy, !same))
}

/// True if the agent of the home is loaded in launchd, or (Windows) its Startup script exists.
pub fn is_loaded(home: &Path) -> bool {
    if cfg!(windows) {
        return agent_file(home).is_ok_and(|file| file.exists());
    }
    let Ok(domain) = gui_domain() else { return false };
    Command::new("launchctl").args(["print", &format!("{domain}/{}", label(home))]).output().is_ok_and(|o| o.status.success())
}

/// Writes the agent file and loads it. A running daemon must be stopped first, because the new
/// daemon needs the database lock.
pub fn install(daemon: &Path, home: &Path, dry_run: bool) -> anyhow::Result<()> {
    if cfg!(windows) {
        let file = agent_file(home)?;
        if dry_run {
            println!("{}(dry run: {} was not written)", startup_script(&home.join("bin/zcied.exe"), home), file.display());
            return Ok(());
        }
        let (copy, _) = sync_daemon_copy(daemon, home)?;
        std::fs::create_dir_all(file.parent().context("no Startup folder")?)?;
        std::fs::write(&file, startup_script(&copy, home))?;
        run_startup_script(&file)?;
        println!("installed {} (zcied starts at login)", file.display());
        return Ok(());
    }
    if !cfg!(target_os = "macos") {
        bail!("daemon install supports macOS (launchd) only; on Linux add a systemd user unit for: zcied --home {}", home.display());
    }
    let file = agent_file(home)?;
    if dry_run {
        println!("{}(dry run: {} was not written)", agent_text(&home.join("bin/zcied"), home), file.display());
        return Ok(());
    }
    let (copy, _) = sync_daemon_copy(daemon, home)?;
    std::fs::create_dir_all(file.parent().context("no LaunchAgents folder")?)?;
    std::fs::create_dir_all(home.join("instances/local/telemetry/logs"))?;
    std::fs::write(&file, agent_text(&copy, home))?;
    let domain = gui_domain()?;
    let _ = Command::new("launchctl").args(["bootout", &format!("{domain}/{}", label(home))]).output();
    let status = Command::new("launchctl").args(["bootstrap", &domain]).arg(&file).status()?;
    if !status.success() {
        bail!("launchctl bootstrap {domain} {} failed", file.display());
    }
    println!("installed {} (zcied starts at login and after a crash)", file.display());
    Ok(())
}

/// Restarts the daemon of a loaded agent with the current copy of `zcied`.
pub fn kickstart(daemon: &Path, home: &Path) -> anyhow::Result<()> {
    if cfg!(windows) {
        if super::daemon::is_running(home) {
            super::daemon::stop(home)?;
        }
        sync_daemon_copy(daemon, home)?;
        return run_startup_script(&agent_file(home)?);
    }
    sync_daemon_copy(daemon, home)?;
    let domain = gui_domain()?;
    let status = Command::new("launchctl").args(["kickstart", "-k", &format!("{domain}/{}", label(home))]).status()?;
    if !status.success() {
        bail!("launchctl kickstart failed");
    }
    Ok(())
}

/// Unloads and removes the agent file. The daemon stops.
pub fn uninstall(home: &Path) -> anyhow::Result<()> {
    let file = agent_file(home)?;
    if !file.exists() {
        println!("not installed ({} does not exist)", file.display());
        return Ok(());
    }
    if cfg!(windows) {
        std::fs::remove_file(&file)?;
        println!("removed {}", file.display());
        return Ok(());
    }
    let domain = gui_domain()?;
    let _ = Command::new("launchctl").args(["bootout", &format!("{domain}/{}", label(home))]).output();
    std::fs::remove_file(&file)?;
    println!("removed {}", file.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_27_01_the_startup_script_quotes_paths_with_spaces() {
        let text = startup_script(Path::new(r"C:\Users\A B\.zcie\bin\zcied.exe"), Path::new(r"C:\Users\A B\.zcie"));
        assert_eq!(text, "CreateObject(\"WScript.Shell\").Run \"\"\"C:\\Users\\A B\\.zcie\\bin\\zcied.exe\"\" --home \"\"C:\\Users\\A B\\.zcie\"\"\", 0, False\r\n");
    }
}
