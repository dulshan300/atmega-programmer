//! Path and identifier validation shared by every backend entry point.

use crate::{CoreError, CoreResult};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Requires an absolute path without `..` components.
pub fn require_absolute(path: &Path) -> CoreResult<PathBuf> {
    if !path.is_absolute() {
        return Err(CoreError::InvalidRequest(format!(
            "path must be absolute: {}",
            path.display()
        )));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(CoreError::InvalidRequest(format!(
            "path must not contain '..': {}",
            path.display()
        )));
    }
    Ok(path.to_path_buf())
}

/// Joins a relative path onto a root, refusing anything that could escape it.
pub fn join_inside(root: &Path, relative: &str) -> CoreResult<PathBuf> {
    let rel = Path::new(relative);
    // Check raw segments: `components()` silently drops interior `.` parts.
    let bad_segment = relative
        .split(['/', '\\'])
        .any(|s| s.is_empty() || s == "." || s == "..");
    if rel.is_absolute()
        || bad_segment
        || rel.components().any(|c| !matches!(c, Component::Normal(_)))
        || relative.contains('\\') && cfg!(not(windows))
    {
        return Err(CoreError::InvalidRequest(format!(
            "invalid relative path: {relative}"
        )));
    }
    Ok(root.join(rel))
}

/// Project display names: 1–64 printable characters, no path separators.
pub fn validate_project_name(name: &str) -> CoreResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 64 {
        return Err(CoreError::InvalidRequest(
            "project name must be 1–64 characters".into(),
        ));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Err(CoreError::InvalidRequest(
            "project name must not contain control characters or / \\ : * ? \" < > |".into(),
        ));
    }
    Ok(trimmed.to_string())
}

/// Folder name derived from a project name.
pub fn folder_name_for(name: &str) -> String {
    let mut out: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "avr-project".into() } else { out }
}

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else", "enum",
    "extern", "float", "for", "goto", "if", "inline", "int", "long", "register", "restrict", "return",
    "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned", "void",
    "volatile", "while", "_Bool", "bool", "true", "false", "main", "setup", "loop",
];

/// C identifiers used for variables, functions and pin labels.
pub fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && s.len() <= 32
        && !s.starts_with("__")
        && !C_KEYWORDS.contains(&s)
}

/// Writes a file atomically: write to a temporary sibling, flush, then rename.
/// If `backup` is set and the target exists, the old content is kept as `<name>.bak`.
pub fn atomic_write(path: &Path, contents: &[u8], backup: bool) -> CoreResult<()> {
    let dir = path
        .parent()
        .ok_or_else(|| CoreError::InvalidRequest("path has no parent".into()))?;
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{file_name}.tmp-{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| CoreError::io(&tmp, e))?;
        f.write_all(contents).map_err(|e| CoreError::io(&tmp, e))?;
        f.sync_all().map_err(|e| CoreError::io(&tmp, e))?;
    }
    if backup && path.exists() {
        let bak = dir.join(format!("{file_name}.bak"));
        std::fs::copy(path, &bak).map_err(|e| CoreError::io(&bak, e))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        CoreError::io(path, e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_inside_rejects_escape() {
        let root = Path::new("/tmp/x");
        assert!(join_inside(root, "../etc/passwd").is_err());
        assert!(join_inside(root, "/etc/passwd").is_err());
        assert!(join_inside(root, "a/./b").is_err());
        assert_eq!(
            join_inside(root, "build/fw.hex").unwrap(),
            root.join("build/fw.hex")
        );
    }

    #[test]
    fn identifiers() {
        assert!(is_valid_identifier("led_count"));
        assert!(is_valid_identifier("_x1"));
        assert!(!is_valid_identifier("1abc"));
        assert!(!is_valid_identifier("int"));
        assert!(!is_valid_identifier("__reserved"));
        assert!(!is_valid_identifier("a-b"));
    }

    #[test]
    fn names() {
        assert!(validate_project_name("Blink 1").is_ok());
        assert!(validate_project_name("a/b").is_err());
        assert!(validate_project_name("   ").is_err());
        assert_eq!(folder_name_for("My Blink: test!"), "My-Blink-test");
        assert_eq!(folder_name_for("???"), "avr-project");
    }

    #[test]
    fn atomic_write_keeps_backup() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.json");
        atomic_write(&p, b"one", true).unwrap();
        atomic_write(&p, b"two", true).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.json.bak")).unwrap(),
            "one"
        );
    }
}
