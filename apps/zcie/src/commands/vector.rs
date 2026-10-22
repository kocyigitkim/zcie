//! `zcie vector install|status`: the local embedding model of the vector search (ADR-011).
//! The download is pinned to one model revision and each file is checked with a BLAKE3 hash.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Subcommand;

/// A model that `zcie vector install` can download (Hugging Face, MIT license).
struct ModelSource {
    /// Folder name under `<home>/models/` (one of `zcie_vector::KNOWN_MODELS`).
    name: &'static str,
    /// Repository on Hugging Face.
    repository: &'static str,
    /// Pinned revision (commit) of the repository.
    revision: &'static str,
    /// Files and their BLAKE3 hashes.
    files: [(&'static str, &'static str); 3],
}

/// The known models; the first one is the default.
const MODELS: [ModelSource; 2] = [
    ModelSource {
        name: "potion-multilingual-128M",
        repository: "minishlab/potion-multilingual-128M",
        revision: "73908c3438cf03b6a01bcb9611d62b23d0726f08",
        files: [
            ("config.json", "f5ed92eaf748e6f0e19db16f979e7be1ca8c8b71dfe02326e7963fae4168a72e"),
            ("tokenizer.json", "b3d6fccbcd32ace4482accfe0a6118e8acb95ecb1bc1586d6bdf30169e983282"),
            ("model.safetensors", "bfffe7401597affd16bfcb6da81f254758c847a1ffba24069a1a9af4ad461ca3"),
        ],
    },
    ModelSource {
        name: "potion-base-8M",
        repository: "minishlab/potion-base-8M",
        revision: "bf8b056651a2c21b8d2565580b8569da283cab23",
        files: [
            ("config.json", "fa5039b3c2c49c37a7d7d36ee5990372868b0323693606b749db06797d6f99d6"),
            ("tokenizer.json", "ca033e2ab84c33be3537a08555e3af2986d85eec2e555cbddf12588e39c779e4"),
            ("model.safetensors", "82efbdca782502a13cb343507af0c93329e7c55dcc2894ea6fc61c8780151c0c"),
        ],
    },
];
/// Download timeout of one file.
const DOWNLOAD_TIMEOUT_SECS: u64 = 600;

/// Vector actions.
#[derive(Debug, Subcommand)]
pub enum VectorAction {
    /// Download an embedding model to `<home>/models/`. Restart the daemon after it.
    Install {
        /// potion-multilingual-128M (default, 512 MB download, about 200 MB in memory; Turkish and
        /// English) or potion-base-8M (31 MB; English only).
        #[arg(long, default_value = zcie_vector::DEFAULT_MODEL)]
        model: String,
    },
    /// Show if the model is installed and the daemon can use it.
    Status {
        /// The model to check.
        #[arg(long, default_value = zcie_vector::DEFAULT_MODEL)]
        model: String,
    },
}

/// Runs one vector action.
pub fn run(action: VectorAction, home: &Path) -> anyhow::Result<()> {
    let (VectorAction::Install { model } | VectorAction::Status { model }) = &action;
    let Some(source) = MODELS.iter().find(|m| m.name == model) else {
        bail!("unknown model {model}; known: {}", MODELS.map(|m| m.name).join(", "));
    };
    let folder = home.join("models").join(source.name);
    match action {
        VectorAction::Install { .. } => install(source, &folder, true),
        VectorAction::Status { .. } => status(source, &folder),
    }
}

/// Installs a known model in `<home>/models/<name>` (used by `zcie setup`, which also sets the
/// model in `engine.toml` and restarts the daemon).
pub fn install_model(home: &Path, name: &str) -> anyhow::Result<()> {
    let Some(source) = MODELS.iter().find(|m| m.name == name) else { bail!("unknown model {name}") };
    install(source, &home.join("models").join(name), false)
}

/// True if all files of a known model are in `<home>/models/<name>` (sizes only; fast).
pub fn model_present(home: &Path, name: &str) -> bool {
    let folder = home.join("models").join(name);
    MODELS.iter().find(|m| m.name == name).is_some_and(|m| m.files.iter().all(|(file, _)| folder.join(file).exists()))
}

/// Returns a verified file of another installation (`~/.zcie/models`), so that a second home or a
/// new install does not download the same 512 MB again.
fn local_copy(model: &str, file: &str, expected: &str, target: &Path) -> Option<PathBuf> {
    let other = PathBuf::from(crate::commands::user_home().ok()?).join(".zcie/models").join(model).join(file);
    (other != target && other.exists() && hash(&other).ok()? == expected).then_some(other)
}

fn install(source: &ModelSource, folder: &Path, hint: bool) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder)?;
    for (name, expected) in source.files {
        let target = folder.join(name);
        if target.exists() && hash(&target)? == expected {
            println!("ok          {name}");
            continue;
        }
        if let Some(other) = local_copy(source.name, name, expected, &target) {
            std::fs::copy(&other, &target)?;
            println!("copied      {name} (from {})", other.display());
            continue;
        }
        let url = format!("https://huggingface.co/{}/resolve/{}/{name}", source.repository, source.revision);
        let partial = target.with_extension("part");
        download(&url, &partial)?;
        let actual = hash(&partial)?;
        if actual != expected {
            let _ = std::fs::remove_file(&partial);
            bail!("{name}: BLAKE3 {actual} is not the expected {expected}; the file was not kept");
        }
        std::fs::rename(&partial, &target)?;
        println!("downloaded  {name}");
    }
    zcie_vector::StaticModel::load(folder).context("the model does not load")?;
    if !hint {
        println!("model ready in {}", folder.display());
        return Ok(());
    }
    let config = if source.name == zcie_vector::DEFAULT_MODEL { String::new() } else { format!(" Set [vector] model = \"{}\" in engine.toml.", source.name) };
    println!("Model ready in {}.{config} Restart the daemon: zcie daemon restart", folder.display());
    Ok(())
}

/// Downloads with `curl` (present on macOS and Linux; the hooks use it too).
fn download(url: &str, target: &Path) -> anyhow::Result<()> {
    let status = std::process::Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location", "--max-time", &DOWNLOAD_TIMEOUT_SECS.to_string(), "--output"])
        .arg(target)
        .arg(url)
        .status()
        .context("curl is not installed")?;
    if !status.success() {
        bail!("download failed: {url}");
    }
    Ok(())
}

fn hash(path: &Path) -> anyhow::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hasher.finalize().to_hex().to_string());
        }
        hasher.update(&buffer[..read]);
    }
}

fn status(source: &ModelSource, folder: &Path) -> anyhow::Result<()> {
    for (name, expected) in source.files {
        let path = folder.join(name);
        let state = match path.exists() {
            false => "missing",
            true if hash(&path)? == expected => "ok",
            true => "changed (run: zcie vector install)",
        };
        println!("{state:<10}  {}", path.display());
    }
    match zcie_vector::StaticModel::load(folder) {
        Ok(model) => println!("model loads: {} dimensions. The daemon uses it after a restart if [vector] enabled = true.", model.dim()),
        Err(error) => println!("model does not load: {error}. Run: zcie vector install"),
    }
    Ok(())
}
