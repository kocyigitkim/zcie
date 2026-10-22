//! `zcie setup`, `zcie config` and `zcie uninstall` (ADR-014): the installation wizard, the
//! later change of its settings, and the removal. `zcie verify` checks that everything works.

pub mod apply;
pub mod config;
pub mod live;
pub mod prompt;
pub mod settings;
pub mod verify;

use std::path::Path;

use clap::Args;

use prompt::Prompter;
use settings::Settings;

/// Returns the English or the Turkish text.
pub fn t(lang: &str, en: &'static str, tr: &'static str) -> &'static str {
    if lang == "tr" { tr } else { en }
}

/// Options of `zcie setup`. Each setting can be given as an option; the wizard asks the others.
#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Ask nothing: take the given options, the current settings or the defaults.
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Language of the wizard and the reports: tr or en.
    #[arg(long)]
    pub language: Option<String>,
    /// Claude Code integration: user (all projects), project (one folder) or none.
    #[arg(long)]
    pub claude: Option<String>,
    /// Project folder for `--claude project`.
    #[arg(long)]
    pub claude_dir: Option<String>,
    /// Claude Code tool profile: auto, balanced, lean or off.
    #[arg(long)]
    pub profile: Option<String>,
    /// Token budget of the context capsule (200 to 4000).
    #[arg(long)]
    pub budget: Option<String>,
    /// Vector search: multilingual, english or off.
    #[arg(long)]
    pub vector: Option<String>,
    /// Index workers: 1, 2 or 4.
    #[arg(long)]
    pub workers: Option<String>,
    /// Start the daemon at login (macOS): true or false.
    #[arg(long)]
    pub autostart: Option<String>,
    /// Daemon port.
    #[arg(long)]
    pub port: Option<String>,
    /// OpenCode integration for all projects: true or false.
    #[arg(long)]
    pub opencode: Option<String>,
    /// Cursor integration for all projects: true or false.
    #[arg(long)]
    pub cursor: Option<String>,
    /// Store the answers of the assistant too: true or false.
    #[arg(long)]
    pub answers: Option<String>,
    /// Check the project after code changes (verification gate): auto or off.
    #[arg(long)]
    pub check: Option<String>,
    /// The context hook also reads projects that a prompt names: true or false.
    #[arg(long)]
    pub cross_project: Option<String>,
    /// Import earlier Claude Code conversations: true or false.
    #[arg(long)]
    pub import: Option<String>,
    /// Run the live test with two real Claude Code sessions: true or false.
    #[arg(long)]
    pub live: Option<String>,
    /// Do not run `zcie verify` at the end.
    #[arg(long)]
    pub no_verify: bool,
}

impl SetupArgs {
    fn given(&self) -> Vec<(&'static str, &String)> {
        let pairs = [
            ("language", &self.language),
            ("claude", &self.claude),
            ("claude_dir", &self.claude_dir),
            ("profile", &self.profile),
            ("budget", &self.budget),
            ("vector", &self.vector),
            ("workers", &self.workers),
            ("autostart", &self.autostart),
            ("port", &self.port),
            ("opencode", &self.opencode),
            ("cursor", &self.cursor),
            ("answers", &self.answers),
            ("check", &self.check),
            ("cross_project", &self.cross_project),
        ];
        pairs.into_iter().filter_map(|(k, v)| v.as_ref().map(|v| (k, v))).collect()
    }
}

/// Runs the wizard: settings, apply, import, verify.
pub fn run(args: &SetupArgs, home: &Path) -> anyhow::Result<()> {
    let old = settings::load(home)?;
    let mut new = old.clone().unwrap_or_default();
    let given = args.given();
    for (key, value) in &given {
        new.set(key, value)?;
    }
    let mut prompter = Prompter::new();
    if !args.yes {
        ask_all(&mut prompter, &mut new, &given.iter().map(|(k, _)| *k).collect::<Vec<_>>())?;
    }
    new.validate()?;
    let lang = new.language.clone();
    print_summary(&new, home);
    if !args.yes && !prompter.yes_no(t(&lang, "Apply these settings?", "Bu ayarlar uygulansın mı?"), true) {
        println!("{}", t(&lang, "Nothing was changed.", "Hiçbir şey değiştirilmedi."));
        return Ok(());
    }
    apply::apply(home, old.as_ref(), &new)?;
    import_step(args, &mut prompter, home, &new, old.is_none())?;
    if args.no_verify {
        return Ok(());
    }
    let live = decide(args.live.as_deref(), args.yes, || {
        live::claude_available() && prompter.yes_no(t(&lang, "Run a live test with two new Claude Code sessions (about $0.05)?", "İki yeni Claude Code oturumuyla canlı test yapılsın mı (yaklaşık 0,05 $)?"), true)
    })?;
    verify::run(home, &new, live)
}

/// Returns a yes/no option, its answer in the wizard, or false with `--yes`.
fn decide(option: Option<&str>, yes: bool, ask: impl FnOnce() -> bool) -> anyhow::Result<bool> {
    match option {
        Some(value) => settings::parse_bool(value),
        None if yes => Ok(false),
        None => Ok(ask()),
    }
}

/// Asks the settings that no option gave.
fn ask_all(p: &mut Prompter, s: &mut Settings, given: &[&str]) -> anyhow::Result<()> {
    let skip = |key: &str| given.contains(&key);
    if !skip("language") {
        s.language = p.choose("Dil / Language", &[("tr", "Türkçe"), ("en", "English")], &s.language);
    }
    let l = s.language.clone();
    if !skip("claude") {
        let options = [
            ("user", t(&l, "All projects (recommended)", "Tüm projeler (önerilen)")),
            ("project", t(&l, "Only one folder", "Yalnızca bir klasör")),
            ("none", t(&l, "Do not connect Claude Code", "Claude Code bağlanmasın")),
        ];
        s.claude = p.choose(t(&l, "Claude Code integration (memory in every new session):", "Claude Code entegrasyonu (her yeni oturumda hafıza):"), &options, &s.claude);
    }
    if s.claude == "project" && !skip("claude_dir") {
        let current = if s.claude_dir.is_empty() { std::env::current_dir()?.display().to_string() } else { s.claude_dir.clone() };
        let answer = p.text(t(&l, "Project folder", "Proje klasörü"), &current);
        s.set("claude_dir", &answer)?;
    }
    if s.claude != "none" {
        ask_claude_details(p, s, given);
    }
    ask_engine(p, s, given);
    if !skip("opencode") && (live::on_path("opencode") || s.opencode) {
        s.opencode = p.yes_no(t(&l, "Connect OpenCode too?", "OpenCode da bağlansın mı?"), true);
    }
    let cursor_home = std::env::var("HOME").map(|h| Path::new(&h).join(".cursor").is_dir()).unwrap_or(false);
    if !skip("cursor") && (live::on_path("cursor") || cursor_home || s.cursor) {
        s.cursor = p.yes_no(t(&l, "Connect Cursor too?", "Cursor da bağlansın mı?"), true);
    }
    Ok(())
}

fn ask_claude_details(p: &mut Prompter, s: &mut Settings, given: &[&str]) {
    let l = s.language.clone();
    if !given.contains(&"profile") {
        let options = [
            ("auto", t(&l, "Auto (recommended): rarely used tools load only when needed", "Otomatik (önerilen): az kullanılan araçlar gerektiğinde yüklenir")),
            ("balanced", t(&l, "Balanced: 15 heavy tools off", "Dengeli: 15 ağır araç kapalı")),
            ("lean", t(&l, "Lean: only Bash, Read, Edit, Write", "En az: yalnızca Bash, Read, Edit, Write")),
            ("off", t(&l, "Do not change the tool list", "Araç listesine dokunma")),
        ];
        s.profile = p.choose(t(&l, "Tool profile of Claude Code (token cost):", "Claude Code araç profili (token maliyeti):"), &options, &s.profile);
    }
    if !given.contains(&"answers") {
        s.answers = p.yes_no(t(&l, "Store the answers of the assistant too (recommended)?", "Asistanın yanıtları da hafızaya kaydedilsin mi (önerilen)?"), s.answers);
    }
    if !given.contains(&"check") {
        let on = s.check == "auto";
        let question = t(&l, "Check the project (build or type check) when Claude changed code, and let Claude fix the errors (recommended)?", "Claude kod değiştirdiğinde proje kontrol edilsin (derleme/tip kontrolü) ve hatalar Claude'a düzelttirilsin mi (önerilen)?");
        s.check = if p.yes_no(question, on) { "auto".into() } else { "off".into() };
    }
    if !given.contains(&"budget") {
        let question = t(&l, "Most tokens of memory context added to a prompt", "Bir prompta eklenecek hafıza bağlamının üst sınırı (token)");
        for _ in 0..3 {
            let answer = p.text(question, &s.budget.to_string());
            if s.set("budget", &answer).is_ok() {
                break;
            }
            println!("{}", t(&l, "  Enter a number from 200 to 4000.", "  200 ile 4000 arasında bir sayı girin."));
        }
    }
}

fn ask_engine(p: &mut Prompter, s: &mut Settings, given: &[&str]) {
    let l = s.language.clone();
    if !given.contains(&"vector") {
        let options = [
            ("multilingual", t(&l, "Multilingual (recommended; Turkish and English; 512 MB download, about 200 MB memory)", "Çok dilli (önerilen; Türkçe ve İngilizce; 512 MB indirme, yaklaşık 200 MB bellek)")),
            ("english", t(&l, "English only (31 MB)", "Yalnızca İngilizce (31 MB)")),
            ("off", t(&l, "Off (word search only)", "Kapalı (yalnızca kelime araması)")),
        ];
        s.vector = p.choose(t(&l, "Semantic search:", "Anlamsal arama:"), &options, &s.vector);
    }
    if !given.contains(&"workers") {
        let options = [
            ("1", t(&l, "Low CPU", "Düşük CPU")),
            ("2", t(&l, "Balanced (recommended)", "Dengeli (önerilen)")),
            ("4", t(&l, "Fast (uses more CPU while a project is indexed)", "Hızlı (proje indekslenirken daha çok CPU)")),
        ];
        let answer = p.choose(t(&l, "Background indexing:", "Arka plan indeksleme:"), &options, &s.workers.to_string());
        let _ = s.set("workers", &answer);
    }
    if !given.contains(&"autostart") && cfg!(target_os = "macos") {
        s.autostart = p.yes_no(t(&l, "Start the daemon at login?", "Daemon oturum açılışında otomatik başlasın mı?"), s.autostart);
    }
    if !given.contains(&"port") {
        let answer = p.text(t(&l, "Daemon port", "Daemon portu"), &s.port.to_string());
        if s.set("port", &answer).is_err() {
            println!("{}", t(&l, "  Not a valid port; the old value stays.", "  Geçersiz port; eski değer kaldı."));
        }
    }
}

fn print_summary(s: &Settings, home: &Path) {
    println!("\n{}", t(&s.language, "Settings:", "Ayarlar:"));
    for (key, _) in settings::KEYS {
        let value = s.get(key);
        if !(key == "claude_dir" && value.is_empty()) {
            println!("  {key:<11} = {value}");
        }
    }
    println!("  ({})", settings::file(home).display());
}

/// Imports the earlier Claude Code conversations if the user wants it.
fn import_step(args: &SetupArgs, p: &mut Prompter, home: &Path, s: &Settings, first: bool) -> anyhow::Result<()> {
    let transcripts = std::env::var("HOME").map(|h| Path::new(&h).join(".claude/projects").is_dir()).unwrap_or(false);
    let l = s.language.as_str();
    let question = t(l, "Import your earlier Claude Code conversations into the memory? (the code of a folder is indexed only after: zcie project add <folder>)", "Eski Claude Code sohbetleriniz hafızaya aktarılsın mı? (bir klasörün kodu yalnızca zcie project add <klasör> ile indekslenir)");
    let wanted = decide(args.import.as_deref(), args.yes, || transcripts && p.yes_no(question, first))?;
    if !wanted {
        return Ok(());
    }
    let key = std::fs::read_to_string(home.join("config/admin.key")).unwrap_or_default();
    let client = zcie_sdk::Client::new(&s.url(), key.trim(), "default");
    // The daemon can be busy for some seconds after a restart (pending jobs, vector index).
    crate::commands::daemon::wait_healthy(&client, &crate::commands::daemon::log_file(home)?)?;
    let import = crate::commands::import::ImportArgs { dir: None, include_automated: false, dry_run: false, existing_only: false };
    crate::commands::import::run(import, &client)
}
