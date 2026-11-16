//! Harness's own skills: folders with a SKILL.md (instructions for a kind of
//! task), kept in the app's data folder and offered to every chat's agent,
//! whatever the agent. The user imports them in Settings (a folder, a .zip of
//! one, or a lone .md file) as copies; nothing in an agent's own skill folders
//! is touched. Agents load a skill through the app's `skill` tool, which also
//! reads the skill's other files, so no agent needs access to this folder.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

/// The name the agent knows the tool group by.
pub const SERVER: &str = "harness_skills";
const FILE: &str = "SKILL.md";
/// Largest skill an import copies, all files together.
const MAX_IMPORT_BYTES: u64 = 50 * 1024 * 1024;
/// Largest file the tool hands an agent.
const MAX_READ_BYTES: u64 = 256 * 1024;
/// Longest description shown in the list given to agents.
const SHORT_DESCRIPTION: usize = 300;
/// Most other files listed when a skill is loaded.
const MAX_LISTED: usize = 200;
/// Folders and files never copied in.
const SKIPPED: [&str; 3] = [".DS_Store", ".git", "__MACOSX"];

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

#[derive(Clone)]
pub struct Skills {
    root: PathBuf,
}

impl Skills {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The imported skills, by name.
    pub fn list(&self) -> Vec<SkillInfo> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut skills: Vec<SkillInfo> = entries
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let text = std::fs::read_to_string(e.path().join(FILE)).ok()?;
                let description = parse_header(&text).1.filter(|d| !d.is_empty())?;
                Some(SkillInfo { name, description })
            })
            .collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    /// Copies a skill in from a folder, a .zip or a .md file. A skill with the
    /// same name is replaced: that is how one is updated.
    pub fn import(&self, source: &Path) -> Result<SkillInfo, String> {
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let temp = self.root.join(format!(".importing-{}", uuid::Uuid::new_v4().simple()));
        let result = self.import_into(source, &temp);
        let _ = std::fs::remove_dir_all(&temp);
        result
    }

    fn import_into(&self, source: &Path, temp: &Path) -> Result<SkillInfo, String> {
        let extension = source.extension().map(|e| e.to_string_lossy().to_lowercase());
        // The copied skill's folder, and the name to fall back on.
        let (folder, fallback) = if source.is_dir() {
            if !source.join(FILE).is_file() {
                return Err("No SKILL.md in that folder".to_string());
            }
            let mut budget = MAX_IMPORT_BYTES;
            copy_folder(source, temp, &mut budget)?;
            (temp.to_path_buf(), file_name(source))
        } else if extension.as_deref() == Some("zip") {
            unzip(source, temp)?;
            let folder = find_skill(temp)?;
            let fallback = if folder == temp { stem(source) } else { file_name(&folder) };
            (folder, fallback)
        } else if extension.as_deref() == Some("md") {
            let size = std::fs::metadata(source).map_err(|e| e.to_string())?.len();
            if size > MAX_IMPORT_BYTES {
                return Err(too_big());
            }
            std::fs::create_dir_all(temp).map_err(|e| e.to_string())?;
            std::fs::copy(source, temp.join(FILE)).map_err(|e| e.to_string())?;
            let fallback = stem(source);
            if fallback.eq_ignore_ascii_case("SKILL") {
                let text = std::fs::read_to_string(temp.join(FILE)).map_err(|e| e.to_string())?;
                if parse_header(&text).0.is_none() {
                    return Err("Add a name: line to the file, or import its folder".to_string());
                }
            }
            (temp.to_path_buf(), fallback)
        } else {
            return Err("Pick a skill folder, a .zip or a .md file".to_string());
        };
        let text = std::fs::read_to_string(folder.join(FILE))
            .map_err(|_| "SKILL.md isn't a readable text file".to_string())?;
        let (name, description) = parse_header(&text);
        let description = description
            .filter(|d| !d.is_empty())
            .ok_or_else(|| "SKILL.md has no description".to_string())?;
        let name = clean_name(&name.unwrap_or(fallback)).ok_or_else(|| "The skill has no usable name".to_string())?;
        let target = self.root.join(&name);
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&folder, &target).map_err(|e| e.to_string())?;
        Ok(SkillInfo { name, description })
    }

    /// Deletes an imported skill.
    pub fn remove(&self, name: &str) -> Result<(), String> {
        let folder = self.folder(name)?;
        std::fs::remove_dir_all(folder).map_err(|e| e.to_string())
    }

    /// A skill's folder, if the name is a valid one.
    fn folder(&self, name: &str) -> Result<PathBuf, String> {
        if clean_name(name).as_deref() != Some(name) {
            return Err(format!("Not a skill name: {name}"));
        }
        Ok(self.root.join(name))
    }

    /// What an agent gets when it loads a skill: where it is, its other files,
    /// and its instructions.
    pub fn load(&self, name: &str) -> Result<String, String> {
        let unknown = || {
            let names: Vec<String> = self.list().into_iter().map(|s| s.name).collect();
            let names = if names.is_empty() { "none".to_string() } else { names.join(", ") };
            format!("No Harness skill named {name}. Skills: {names}")
        };
        let folder = self.folder(name).map_err(|_| unknown())?;
        let text = std::fs::read_to_string(folder.join(FILE)).map_err(|_| unknown())?;
        let mut files = Vec::new();
        list_files(&folder, &folder, &mut files);
        files.retain(|f| f != FILE);
        files.sort();
        let others = if files.is_empty() {
            "none".to_string()
        } else {
            let more = files.len().saturating_sub(MAX_LISTED);
            let mut shown = files.into_iter().take(MAX_LISTED).collect::<Vec<_>>().join("\n");
            if more > 0 {
                shown.push_str(&format!("\n…and {more} more"));
            }
            format!("\n{shown}\n(Read them with the {SERVER} skill tool's `file` argument.)")
        };
        Ok(format!("Skill \"{name}\", folder: {}\nOther files: {others}\n\n{text}", folder.display()))
    }

    /// One of a skill's other files, as text.
    pub fn read_file(&self, name: &str, file: &str) -> Result<String, String> {
        let folder = self.folder(name)?;
        let relative = Path::new(file);
        if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(format!("Give a path inside the skill, like references/guide.md, not {file}"));
        }
        let path = folder.join(relative);
        let real = path.canonicalize().map_err(|_| format!("No file {file} in skill {name}"))?;
        let real_folder = folder.canonicalize().map_err(|e| e.to_string())?;
        if !real.starts_with(&real_folder) || !real.is_file() {
            return Err(format!("No file {file} in skill {name}"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&real)
            .and_then(|f| f.take(MAX_READ_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_READ_BYTES {
            return Err(format!("{file} is too big to read here (over 256 KB); open it at {}", real.display()));
        }
        String::from_utf8(bytes).map_err(|_| format!("Not a text file; open it at {}", real.display()))
    }

    /// Runs one of the group's tools.
    pub async fn call(&self, tool: &str, args: &Value) -> Result<Vec<Value>, String> {
        if tool != "skill" {
            return Err(format!("No such tool: {tool}"));
        }
        let name = args["name"].as_str().ok_or("Give the skill's name")?;
        let text = match args["file"].as_str().filter(|f| !f.trim().is_empty()) {
            Some(file) => self.read_file(name, file.trim())?,
            None => self.load(name)?,
        };
        Ok(vec![json!({ "type": "text", "text": text })])
    }

    /// A typed `/name request` for an imported skill, rewritten so the agent
    /// gets the skill with it. None when the text isn't one.
    pub fn expand(&self, text: &str) -> Option<String> {
        let rest = text.trim().strip_prefix('/')?;
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (name, request) = rest.split_at(end);
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        let skill = self.load(name).ok()?;
        let request = request.trim();
        let request = if request.is_empty() { "(none given)" } else { request };
        Some(format!("Use this Harness skill for the request below.\n\n{skill}\n\nRequest: {request}"))
    }
}

/// The line added to the agent's instructions: the skills, and when to load one.
pub fn instruction(skills: &[SkillInfo]) -> String {
    format!(
        "Harness skills (load one with the {SERVER} skill tool before a task that matches it):\n{}",
        skill_lines(skills)
    )
}

/// The group's tool definitions.
pub fn tools(skills: &[SkillInfo]) -> Value {
    json!([{
        "name": "skill",
        "description": format!(
            "Load a Harness skill: instructions for a kind of task. Call it before starting a task \
that matches one. Give `file` to read one of the skill's other files instead. Skills:\n{}",
            skill_lines(skills)
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "The skill's name." },
                "file": { "type": "string", "description": "A file inside the skill, like references/guide.md. Leave out to load the skill." },
            },
            "required": ["name"],
        },
    }])
}

fn skill_lines(skills: &[SkillInfo]) -> String {
    skills
        .iter()
        .map(|s| format!("- {}: {}", s.name, short(&s.description)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn short(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(SHORT_DESCRIPTION) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text,
    }
}

fn too_big() -> String {
    "Skill is too big (over 50 MB)".to_string()
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn skipped(name: &str) -> bool {
    SKIPPED.contains(&name)
}

/// A name made safe for a folder: lowercase letters, digits and dashes, at
/// most 64 characters. None if nothing is left.
pub fn clean_name(raw: &str) -> Option<String> {
    let mut name = String::new();
    for c in raw.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            name.push(c);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    let name: String = name.trim_matches('-').chars().take(64).collect();
    let name = name.trim_end_matches('-').to_string();
    (!name.is_empty()).then_some(name)
}

/// Copies a folder, skipping symlinks and junk, within a size budget.
fn copy_folder(from: &Path, to: &Path, budget: &mut u64) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name();
        if skipped(&name.to_string_lossy()) {
            continue;
        }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            copy_folder(&entry.path(), &to.join(&name), budget)?;
        } else if kind.is_file() {
            let size = entry.metadata().map_err(|e| e.to_string())?.len();
            *budget = budget.checked_sub(size).ok_or_else(too_big)?;
            std::fs::copy(entry.path(), to.join(&name)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Unpacks a zip, skipping symlinks, junk and paths that leave the folder,
/// within the size budget (counted as written, not as the zip claims).
fn unzip(source: &Path, to: &Path) -> Result<(), String> {
    let file = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| "That isn't a readable .zip".to_string())?;
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut budget = MAX_IMPORT_BYTES;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(relative) = entry.enclosed_name() else { continue };
        if entry.is_symlink() || relative.components().any(|c| skipped(&c.as_os_str().to_string_lossy())) {
            continue;
        }
        let path = to.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let written = std::io::copy(&mut (&mut entry).take(budget + 1), &mut out).map_err(|e| e.to_string())?;
        budget = budget.checked_sub(written).ok_or_else(too_big)?;
    }
    Ok(())
}

/// The folder holding the shallowest SKILL.md in an unpacked zip.
fn find_skill(root: &Path) -> Result<PathBuf, String> {
    let mut level = vec![root.to_path_buf()];
    while !level.is_empty() {
        let found: Vec<&PathBuf> = level.iter().filter(|f| f.join(FILE).is_file()).collect();
        match found.len() {
            0 => {}
            1 => return Ok(found[0].clone()),
            _ => return Err("The zip holds more than one skill".to_string()),
        }
        level = level
            .iter()
            .filter_map(|f| std::fs::read_dir(f).ok())
            .flat_map(|entries| entries.flatten())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
    }
    Err("No SKILL.md in that zip".to_string())
}

/// A skill's files, as paths relative to its folder, skipping symlinks.
fn list_files(base: &Path, folder: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            list_files(base, &entry.path(), out);
        } else if kind.is_file()
            && let Ok(relative) = entry.path().strip_prefix(base)
        {
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The `name` and `description` from a SKILL.md's header (the block between
/// `---` lines at the top). Handles plain, quoted and block (`>`, `|`) values,
/// which is what skill headers use; not a full YAML reader.
pub fn parse_header(text: &str) -> (Option<String>, Option<String>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return (None, None);
    }
    let header: Vec<&str> = lines.take_while(|l| l.trim_end() != "---").collect();
    let (mut name, mut description) = (None, None);
    let mut i = 0;
    while i < header.len() {
        let line = header[i];
        i += 1;
        let Some((key, value)) = line.split_once(':') else { continue };
        if key.starts_with(char::is_whitespace) || !matches!(key, "name" | "description") {
            continue;
        }
        let value = value.trim();
        // The lines under the key that belong to it.
        let mut more = Vec::new();
        while i < header.len() && (header[i].trim().is_empty() || header[i].starts_with(char::is_whitespace)) {
            more.push(header[i].trim());
            i += 1;
        }
        let value = if value.starts_with('>') || value.starts_with('|') {
            let joiner = if value.starts_with('>') { " " } else { "\n" };
            more.join(joiner)
        } else {
            let mut all = vec![value];
            all.extend(more);
            let joined = all.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
            unquote(&joined)
        };
        let value = value.trim().to_string();
        if key == "name" {
            name = Some(value).filter(|v| !v.is_empty());
        } else {
            description = Some(value);
        }
    }
    (name, description)
}

fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            let inner = &value[1..value.len() - 1];
            return if quote == '\'' { inner.replace("''", "'") } else { inner.replace("\\\"", "\"") };
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("harness-skills-{label}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn skill_md(name: &str, description: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nDo the thing.\n")
    }

    fn zip_of(path: &Path, files: &[(&str, &str)]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, text) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn headers_of_every_shape_are_read() {
        assert_eq!(
            parse_header("---\nname: a\ndescription: plain words\n---\nbody"),
            (Some("a".into()), Some("plain words".into()))
        );
        assert_eq!(
            parse_header("---\nname: \"a\"\ndescription: 'it''s quoted'\n---\n"),
            (Some("a".into()), Some("it's quoted".into()))
        );
        assert_eq!(
            parse_header("---\nname: b\ndescription: >-\n  folded over\n  two lines\nother: x\n---\n"),
            (Some("b".into()), Some("folded over two lines".into()))
        );
        assert_eq!(
            parse_header("---\ndescription: |\n  line one\n  line two\n---\n"),
            (None, Some("line one\nline two".into()))
        );
        assert_eq!(parse_header("# no header\n"), (None, None));
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(clean_name("Update KB!").as_deref(), Some("update-kb"));
        assert_eq!(clean_name("  --pr_to video-- ").as_deref(), Some("pr-to-video"));
        assert_eq!(clean_name("../..").as_deref(), None);
        assert_eq!(clean_name(&"a".repeat(80)).map(|n| n.len()), Some(64));
    }

    #[test]
    fn a_folder_is_copied_in_without_junk() {
        let (root, src) = (temp("root"), temp("src"));
        let source = src.join("Update KB");
        std::fs::create_dir_all(source.join("references")).unwrap();
        std::fs::create_dir_all(source.join(".git")).unwrap();
        std::fs::write(source.join(FILE), "---\ndescription: Docs\n---\nbody").unwrap();
        std::fs::write(source.join("references/a.md"), "ref").unwrap();
        std::fs::write(source.join(".DS_Store"), "x").unwrap();
        let skills = Skills::new(root.clone());
        let info = skills.import(&source).unwrap();
        assert_eq!(info, SkillInfo { name: "update-kb".into(), description: "Docs".into() });
        assert!(root.join("update-kb/references/a.md").is_file());
        assert!(!root.join("update-kb/.DS_Store").exists());
        assert!(!root.join("update-kb/.git").exists());
        assert_eq!(skills.list(), vec![info]);
        // No temp folder is left behind.
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn zips_with_and_without_a_top_folder() {
        let (root, src) = (temp("root"), temp("src"));
        let skills = Skills::new(root.clone());
        let nested = src.join("nested.zip");
        zip_of(
            &nested,
            &[
                ("kb/SKILL.md", "---\ndescription: d\n---\n"),
                ("kb/agents/x.md", "x"),
                ("__MACOSX/kb/._SKILL.md", "junk"),
            ],
        );
        assert_eq!(skills.import(&nested).unwrap().name, "kb");
        assert!(root.join("kb/agents/x.md").is_file());
        assert!(!root.join("kb/__MACOSX").exists());

        let flat = src.join("Flat One.zip");
        zip_of(&flat, &[("SKILL.md", "---\ndescription: d\n---\n")]);
        assert_eq!(skills.import(&flat).unwrap().name, "flat-one");

        let two = src.join("two.zip");
        zip_of(&two, &[("a/SKILL.md", "---\ndescription: d\n---\n"), ("b/SKILL.md", "---\ndescription: d\n---\n")]);
        assert!(skills.import(&two).unwrap_err().contains("more than one"));
    }

    #[test]
    fn a_zip_cannot_write_outside_its_folder() {
        let (root, src) = (temp("root"), temp("src"));
        let evil = src.join("evil.zip");
        zip_of(&evil, &[("SKILL.md", &skill_md("evil", "d")), ("../escaped.md", "x")]);
        Skills::new(root.clone()).import(&evil).unwrap();
        assert!(!root.join("escaped.md").exists());
        assert!(!src.join("escaped.md").exists());
    }

    #[test]
    fn lone_files_and_bad_skills() {
        let (root, src) = (temp("root"), temp("src"));
        let skills = Skills::new(root.clone());
        let md = src.join("My Skill.md");
        std::fs::write(&md, "---\ndescription: d\n---\n").unwrap();
        assert_eq!(skills.import(&md).unwrap().name, "my-skill");
        assert!(root.join("my-skill/SKILL.md").is_file());

        let bare = src.join("SKILL.md");
        std::fs::write(&bare, "---\ndescription: d\n---\n").unwrap();
        assert!(skills.import(&bare).unwrap_err().contains("name:"));

        let no_description = src.join("x.md");
        std::fs::write(&no_description, "---\nname: x\n---\n").unwrap();
        assert_eq!(skills.import(&no_description).unwrap_err(), "SKILL.md has no description");

        let empty = src.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(skills.import(&empty).unwrap_err(), "No SKILL.md in that folder");

        let other = src.join("x.txt");
        std::fs::write(&other, "x").unwrap();
        assert!(skills.import(&other).unwrap_err().starts_with("Pick a skill"));
        assert_eq!(skills.list().len(), 1);
    }

    #[test]
    fn importing_again_replaces_and_remove_deletes() {
        let (root, src) = (temp("root"), temp("src"));
        let skills = Skills::new(root.clone());
        let md = src.join("kb.md");
        std::fs::write(&md, skill_md("kb", "old")).unwrap();
        skills.import(&md).unwrap();
        std::fs::write(&md, skill_md("kb", "new")).unwrap();
        skills.import(&md).unwrap();
        assert_eq!(skills.list(), vec![SkillInfo { name: "kb".into(), description: "new".into() }]);
        assert!(skills.remove("../kb").is_err());
        skills.remove("kb").unwrap();
        assert!(skills.list().is_empty());
    }

    #[test]
    fn loading_lists_other_files_and_reads_them_safely() {
        let (root, src) = (temp("root"), temp("src"));
        std::fs::write(root.join("secret.md"), "outside").unwrap();
        let source = src.join("kb");
        std::fs::create_dir_all(source.join("references")).unwrap();
        std::fs::write(source.join(FILE), skill_md("kb", "d")).unwrap();
        std::fs::write(source.join("references/guide.md"), "the guide").unwrap();
        let skills = Skills::new(root.clone());
        skills.import(&source).unwrap();

        let loaded = skills.load("kb").unwrap();
        assert!(loaded.contains("references/guide.md"));
        assert!(loaded.contains("Do the thing."));
        assert!(skills.load("nope").unwrap_err().contains("Skills: kb"));

        assert_eq!(skills.read_file("kb", "references/guide.md").unwrap(), "the guide");
        assert!(skills.read_file("kb", "../secret.md").is_err());
        assert!(skills.read_file("kb", &root.join("secret.md").to_string_lossy()).is_err());
        assert!(skills.read_file("kb", "missing.md").is_err());
    }

    #[test]
    fn typed_names_expand_only_for_known_skills() {
        let (root, src) = (temp("root"), temp("src"));
        let skills = Skills::new(root);
        let md = src.join("kb.md");
        std::fs::write(&md, skill_md("kb", "d")).unwrap();
        skills.import(&md).unwrap();
        let expanded = skills.expand("/kb  fix the docs\nplease").unwrap();
        assert!(expanded.contains("Do the thing."));
        assert!(expanded.ends_with("Request: fix the docs\nplease"));
        assert!(skills.expand("/kb").unwrap().ends_with("Request: (none given)"));
        assert_eq!(skills.expand("/compact"), None);
        assert_eq!(skills.expand("kb please"), None);
        assert_eq!(skills.expand("/../kb"), None);
    }

    #[test]
    fn the_tool_is_declared_and_lists_skills() {
        let list = vec![SkillInfo { name: "kb".into(), description: "x".repeat(400) }];
        let tools = tools(&list);
        assert_eq!(tools[0]["inputSchema"]["type"], "object");
        let description = tools[0]["description"].as_str().unwrap();
        assert!(description.contains("- kb: "));
        assert!(description.ends_with('…'));
        assert!(instruction(&list).contains("- kb: "));
    }

    #[tokio::test]
    async fn calls_load_or_read_files() {
        let (root, src) = (temp("root"), temp("src"));
        let source = src.join("kb");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join(FILE), skill_md("kb", "d")).unwrap();
        std::fs::write(source.join("extra.md"), "extra").unwrap();
        let skills = Skills::new(root);
        skills.import(&source).unwrap();
        let loaded = skills.call("skill", &json!({ "name": "kb" })).await.unwrap();
        assert!(loaded[0]["text"].as_str().unwrap().contains("Do the thing."));
        let file = skills.call("skill", &json!({ "name": "kb", "file": "extra.md" })).await.unwrap();
        assert_eq!(file[0]["text"], "extra");
        assert!(skills.call("other", &json!({})).await.is_err());
    }
}
