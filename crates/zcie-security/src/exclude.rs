//! Exclusion globs (`docs/11-security.md` section 6).

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::error::{Error, Result};

/// Default exclusion globs (`docs/11-security.md` section 6).
pub const DEFAULT_GLOBS: [&str; 13] = [
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/*.p12",
    "**/id_rsa*",
    "**/id_ed25519*",
    "**/.aws/**",
    "**/.ssh/**",
    "**/secrets/**",
    "**/node_modules/**",
    "**/target/**",
    "**/.git/**",
];

/// A set of exclusion globs. Paths are relative, with `/` separators.
#[derive(Debug, Clone)]
pub struct ExcludeSet {
    set: GlobSet,
}

impl ExcludeSet {
    /// Builds a set from configured globs. `.git` is always excluded.
    pub fn new(globs: &[String]) -> Result<ExcludeSet> {
        let mut builder = GlobSetBuilder::new();
        for pattern in [".git/**", "**/.git/**", ".git"] {
            add_glob(&mut builder, pattern)?;
        }
        for glob in globs {
            add_expanded(&mut builder, glob)?;
        }
        let set = builder.build().map_err(|e| Error::Glob(e.to_string()))?;
        Ok(ExcludeSet { set })
    }

    /// Builds a set from the default globs and extra globs.
    pub fn with_defaults(extra: &[String]) -> Result<ExcludeSet> {
        let mut all: Vec<String> = DEFAULT_GLOBS.iter().map(|g| g.to_string()).collect();
        all.extend(extra.iter().cloned());
        ExcludeSet::new(&all)
    }

    /// Returns true if the relative path matches a glob.
    pub fn is_excluded(&self, rel_path: &str) -> bool {
        let path = rel_path.trim_start_matches("./").replace('\\', "/");
        self.set.is_match(&path)
    }
}

fn add_expanded(builder: &mut GlobSetBuilder, glob: &str) -> Result<()> {
    let glob = glob.trim().trim_end_matches('/');
    if glob.is_empty() {
        return Ok(());
    }
    add_glob(builder, glob)?;
    if !glob.starts_with("**/") {
        add_glob(builder, &format!("**/{glob}"))?;
    }
    if !glob.ends_with("/**") {
        add_glob(builder, &format!("{glob}/**"))?;
        if !glob.starts_with("**/") {
            add_glob(builder, &format!("**/{glob}/**"))?;
        }
    }
    Ok(())
}

fn add_glob(builder: &mut GlobSetBuilder, pattern: &str) -> Result<()> {
    let glob = Glob::new(pattern).map_err(|e| Error::Glob(e.to_string()))?;
    builder.add(glob);
    Ok(())
}
