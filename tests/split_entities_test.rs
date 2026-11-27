use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SOURCES: &[(&str, &str)] = &[
    ("schema.vdl", r#"schema "split" {
    type "Container" {
        field "title" text heading
        field "version" text optional
        field "featured" ref "Page" compose optional
        field "includes" ref "Page" compose optional
        field "body" body compose ["Page"] optional
    }
    type "Page" {
        field "title" text heading optional
        field "body" body optional
        field "related" ref "Page" compose optional
        field "mixed" ref ["Page", "Verse"] inline optional
    }
    type "Verse" {
        field "body" body optional
        field "original_text" text optional
        field "translation" text optional
        field "locator" text optional
    }
}"#),
    ("container.vdl", r#"package demo
Container include-root {
    title "Include root"
    body {
        include first
        include second
    }
}
Container list-root {
    title "List root"
    version "3"
    featured [second]
    includes [first, second]
}
"#),
    ("pages/items.vdl", r#"package demo
Page first { title "First" body """Arabic: بِسْمِ اللَّهِ""" related [second] mixed [second, prose """between""", quran.30.30] }
Page second { title "Second" body """Second body""" }
Verse "quran.30.30" { body """Arabic evidence""" original_text "فِطْرَتَ اللَّهِ" translation "The fitrah of Allah" locator "30:30" }
"#),
];

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(&root).unwrap();
    for (name, text) in SOURCES {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fs::write(root.join("vdl.toml"), "[build]\nout_dir = \"dist\"\n\n[[build.markdown]]\nroot = \"demo.list-root\"\noutput = \"flat.md\"\n").unwrap();
    (temp, root)
}

fn compile(root: &Path, split: bool) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vdl"));
    command.arg("compile").arg(root);
    if split { command.arg("--split-entities"); }
    command.output().unwrap()
}

#[test]
fn split_e2e_covers_pages_links_order_unicode_and_flat_parity() {
    let (_temp, root) = fixture();
    let flat = compile(&root, false);
    assert!(flat.status.success(), "{}", String::from_utf8_lossy(&flat.stderr));
    let baseline = fs::read(root.join("dist/flat.md")).unwrap();
    let split = compile(&root, true);
    assert!(split.status.success(), "{}", String::from_utf8_lossy(&split.stderr));
    assert_eq!(baseline, fs::read(root.join("dist/flat.md")).unwrap());

    let pages_root = root.join("dist/entities");
    let pages = markdown_files(&pages_root);
    assert_eq!(pages.len(), 5);
    for path in &pages {
        let text = fs::read_to_string(path).unwrap();
        let id = text.lines().find_map(|line| line.strip_prefix("id: \"").and_then(|value| value.strip_suffix('"'))).unwrap();
        assert_eq!(path.file_name().unwrap().to_string_lossy(), format!("{}.md", id));
        assert_eq!(path.parent().unwrap().file_name().unwrap().to_string_lossy(), id.split('.').next().unwrap());
        assert!(path.to_string_lossy().len() < 260);
    }
    let list_path = pages.iter().find(|path| path.ends_with("demo.list-root.md")).unwrap();
    let list = fs::read_to_string(list_path).unwrap();
    let contents = list.split("## includes").nth(1).unwrap();
    assert!(contents.find("1. [First]").unwrap() < contents.find("2. [Second]").unwrap());
    assert!(list.contains("## featured\n\n1. [Second]"));
    assert!(list.contains("version: \"3\""));
    let body_path = pages.iter().find(|path| path.ends_with("demo.include-root.md")).unwrap();
    let body = fs::read_to_string(body_path).unwrap();
    let body_index = body.split("## Body includes").nth(1).unwrap();
    assert!(body_index.find("1. [First]").unwrap() < body_index.find("2. [Second]").unwrap());
    let verse_path = pages.iter().find(|path| path.ends_with("quran.30.30.md")).unwrap();
    let verse = fs::read_to_string(verse_path).unwrap();
    assert!(verse.contains("فِطْرَتَ اللَّهِ") && verse.contains("original_text") && verse.contains("translation") && verse.contains("locator"));
    let first_path = pages.iter().find(|path| path.ends_with("demo.first.md")).unwrap();
    let first = fs::read_to_string(first_path).unwrap();
    assert!(first.contains("[Second]") && first.contains("between") && first.contains("[quran.30.30]"));
    let include_nav = first.lines().find(|line| line.contains("Navigation [Include root]")).unwrap();
    assert!(include_nav.contains("[Second]") && include_nav.contains("→"));
    let list_nav_for_first = first.lines().find(|line| line.contains("Navigation [List root]")).unwrap();
    assert!(list_nav_for_first.contains("[Second]") && list_nav_for_first.contains("←"));
    assert!(!first.lines().filter(|line| line.starts_with("**Navigation")).any(|line| line.contains("quran.30.30")));
    let second_path = pages.iter().find(|path| path.ends_with("demo.second.md")).unwrap();
    let second = fs::read_to_string(second_path).unwrap();
    assert!(!second.contains("version:"));
    let previous_nav = second.lines().find(|line| line.contains("Navigation [Include root]")).unwrap();
    assert!(previous_nav.contains("[First]") && previous_nav.contains("←"));
    let list_nav_for_second = second.lines().find(|line| line.contains("Navigation [List root]")).unwrap();
    assert!(list_nav_for_second.contains("[First]") && list_nav_for_second.contains("→"));
    assert!(pages.iter().any(|path| path.ends_with("demo.second.md")));
    assert_all_links_resolve(&pages_root, &pages);
}

#[test]
fn split_e2e_rejects_case_insensitive_filename_collisions() {
    let (_temp, root) = fixture();
    let path = root.join("pages/items.vdl");
    let mut sources = fs::read_to_string(&path).unwrap();
    sources.push_str("\nPage \"demo.FOO\" { title \"Upper\" body \"\"\"Body\"\"\" }\nPage \"demo.foo\" { title \"Lower\" body \"\"\"Body\"\"\" }\n");
    fs::write(path, sources).unwrap();
    let output = compile(&root, true);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("collision"));
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    fn visit(path: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() { visit(&entry.path(), files); }
            else if entry.path().extension().is_some_and(|extension| extension == "md") { files.push(entry.path()); }
        }
    }
    let mut files = Vec::new();
    visit(root, &mut files);
    files
}

fn assert_all_links_resolve(root: &Path, files: &[PathBuf]) {
    for file in files {
        let text = fs::read_to_string(file).unwrap();
        for link in text.split("](").skip(1) {
            if let Some(target) = link.split(')').next() {
                let decoded = target.replace("%20", " ").replace("%25", "%");
                assert!(file.parent().unwrap().join(decoded).is_file(), "unresolved link from {}: {} under {}", file.display(), target, root.display());
            }
        }
    }
}
