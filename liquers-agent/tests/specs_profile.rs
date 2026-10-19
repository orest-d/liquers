//! The "liquers-specs" profile over a copy of this repository's `specs/`, `memory/` and skills
//! (`specs/design/agent-memory-mvp/phase3-examples.md`, "specs_profile.rs").

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use liquers_agent::{build_server_with, AgentConfig, AgentEnvironment};
use liquers_core::error::Error;
use liquers_core::metadata::Version;
use liquers_mcp::{McpServer, ToolOutput};
use serde_json::{json, Map, Value};

type Res = Result<(), Box<dyn std::error::Error>>;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

struct Copy {
    dir: PathBuf,
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// `specs/`, `memory/`, `.claude/skills/` and `scripts/docs_index.py`, copied to a temporary
/// folder; the profile's relative paths are made absolute there.
fn copy(name: &str) -> Result<Copy, Box<dyn std::error::Error>> {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "liquers-specs-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let repo = repo();
    copy_dir(&repo.join("specs"), &dir.join("specs"))?;
    copy_dir(&repo.join("memory"), &dir.join("memory"))?;
    copy_dir(&repo.join(".claude/skills"), &dir.join(".claude/skills"))?;
    std::fs::create_dir_all(dir.join("scripts"))?;
    std::fs::copy(repo.join("scripts/docs_index.py"), dir.join("scripts/docs_index.py"))?;
    Ok(Copy { dir })
}

fn config(copy: &Copy) -> Result<AgentConfig, Error> {
    let text = std::fs::read_to_string(copy.dir.join("memory/liquers-specs.yaml"))
        .map_err(|e| Error::general_error(e.to_string()))?;
    let root = copy.dir.display().to_string();
    let text = text
        .replace("path: \"./memory\"", &format!("path: \"{root}/memory\""))
        .replace("path: \".\"", &format!("path: \"{root}\""));
    AgentConfig::from_yaml(&text)
}

async fn start_server(copy: &Copy) -> Result<McpServer<AgentEnvironment>, Error> {
    build_server_with(&config(copy)?, false).await
}

fn args(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn rows(output: &ToolOutput) -> Result<Vec<Map<String, Value>>, Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(&output.text)?;
    Ok(value
        .as_array()
        .map(|a| a.iter().filter_map(|r| r.as_object().cloned()).collect())
        .unwrap_or_default())
}

fn text_field(row: &Map<String, Value>, name: &str) -> String {
    row.get(name).and_then(Value::as_str).unwrap_or("").to_string()
}

fn version_note(output: &ToolOutput) -> String {
    output
        .notes
        .iter()
        .find_map(|n| n.strip_prefix("version ").map(str::to_string))
        .unwrap_or_default()
}

/// Every file under `dir` with a hash of its bytes.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Version> {
    let mut out = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(read) = std::fs::read_dir(&folder) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.insert(path.strip_prefix(dir).unwrap_or(&path).to_path_buf(), Version::from_content(&bytes));
            }
        }
    }
    out
}

#[tokio::test]
async fn specs_profile_runs_the_scenarios() -> Res {
    let c = copy("scenarios")?;
    std::fs::write(
        c.dir.join("memory/notes/plain.md"),
        "# A plain note\n\nIts first paragraph is its description.\n",
    )?;
    let before = snapshot(&c.dir);
    let server = start_server(&c).await?;

    // AC-15: orientation.
    let overview = server.call("memory_overview", Map::new()).await?;
    assert!(overview.text.contains("# Liquers specs"), "{}", overview.text);
    assert!(overview.text.contains("| issues | specs/issues |"), "{}", overview.text);
    let describe = server
        .call("memory_describe", args(json!({"collection": "issues"})))
        .await?;
    assert!(describe.text.contains("priority") && describe.text.contains("P0"), "{}", describe.text);

    // AC-1: tiered listing.
    let listed = server
        .call("memory_list", args(json!({"path": "specs/issues", "limit": 1000, "format": "json"})))
        .await?;
    let issues = rows(&listed)?;
    assert!(issues.len() > 100);
    for row in issues.iter().filter(|r| text_field(r, "kind") != "dir") {
        assert!(!text_field(row, "l0").is_empty(), "{row:?}");
    }

    // AC-17: fallback tiers for a document without front-matter.
    let notes = rows(&server.call("memory_list", args(json!({"path": "notes", "format": "json"}))).await?)?;
    let plain = notes
        .iter()
        .find(|r| text_field(r, "key") == "notes/plain.md")
        .ok_or("plain note not listed")?;
    assert_eq!(text_field(plain, "l0"), "A plain note");
    assert_eq!(text_field(plain, "l1"), "Its first paragraph is its description.");

    // AC-2: ranked search, closed issues filtered out before ranking.
    let found = rows(
        &server
            .call(
                "memory_search",
                args(json!({"path": "specs/issues", "text": "expiration", "filter": "-status:closed*",
                            "limit": 5, "format": "json"})),
            )
            .await?,
    )?;
    assert!(!found.is_empty() && found.len() <= 5);
    let scores: Vec<f64> = found.iter().filter_map(|r| r.get("score").and_then(Value::as_f64)).collect();
    assert!(scores.windows(2).all(|w| w[0] >= w[1]), "{scores:?}");
    for row in &found {
        let text = std::fs::read_to_string(c.dir.join(text_field(row, "key")))?;
        assert!(!text.contains("\nstatus: closed"), "{}", text_field(row, "key"));
    }

    // AC-3: section read with the version.
    let issue = "specs/issues/LOG-ENTRY-HAS-NO-SOURCE.md";
    let section = server
        .call("memory_read", args(json!({"key": issue, "section": "Problem"})))
        .await?;
    assert!(section.text.starts_with("## Problem"), "{}", section.text);
    assert!(!section.text.contains("## Impact"));
    let version = version_note(&section);
    assert_eq!(version.len(), 32);

    // AC-6: a stale precondition is refused and changes nothing.
    let stale = server
        .call(
            "memory_edit",
            args(json!({"key": issue, "old": "Found while building", "new": "Found while writing",
                        "expected_version": "0".repeat(32), "reason": "test"})),
        )
        .await;
    assert!(stale.is_err());
    assert_eq!(Version::from_content(&std::fs::read(c.dir.join(issue))?).to_string(), version);

    // AC-16 and AC-7: replace, set a status along an allowed transition, refuse a forbidden one.
    server
        .call(
            "memory_edit",
            args(json!({"key": issue, "old": "Found while building", "new": "Found while writing",
                        "expected_version": version, "reason": "test replace"})),
        )
        .await?;
    server
        .call("memory_edit", args(json!({"key": issue, "field": "status", "value": "in_progress", "reason": "test"})))
        .await?;
    let forbidden = server
        .call("memory_edit", args(json!({"key": issue, "field": "status", "value": "draft", "reason": "test"})))
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(forbidden.contains("in_progress → draft"), "{forbidden}");
    let edited = std::fs::read_to_string(c.dir.join(issue))?;
    assert!(edited.contains("Found while writing") && edited.contains("status: in_progress"));
    assert!(edited.contains("\nupdated: "), "{edited}");

    let reference = "specs/reference/STORE_SEMANTICS.md";
    server
        .call(
            "memory_edit",
            args(json!({"key": reference, "section": "History",
                        "row": "| 2026-10-19 | Test row. | agent-memory-mvp |", "reason": "test"})),
        )
        .await?;
    let referenced = std::fs::read_to_string(c.dir.join(reference))?;
    assert!(referenced.contains("| 2026-10-19 | Test row. | agent-memory-mvp |"));
    let log_dir = c.dir.join("memory/_log");
    let log: String = std::fs::read_dir(&log_dir)?
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .collect();
    assert_eq!(log.lines().filter(|l| l.contains(issue)).count(), 2, "{log}");
    assert_eq!(log.lines().filter(|l| l.contains(reference)).count(), 1, "{log}");

    // AC-11, the file edited outside the service and read by a server that has not loaded it.
    // (A server that has loaded the document keeps serving its copy; see the AC-11 decision in
    // phase4-implementation.md.)
    let outside = "specs/issues/RECIPE-RESULT-UNDER-STORAGE-NONE-READS-AS-SOURCE.md";
    let original = std::fs::read_to_string(c.dir.join(outside))?;
    let changed = original.replacen("title: A recipe result", "title: Edited by hand, a recipe result", 1);
    std::fs::write(c.dir.join(outside), &changed)?;
    let fresh = start_server(&c).await?;
    let l0 = fresh.call("memory_read", args(json!({"key": outside, "tier": "l0"}))).await?;
    assert!(l0.text.starts_with("Edited by hand"), "{}", l0.text);
    std::fs::write(c.dir.join(outside), &original)?;

    // AC-13: the archive reads and refuses writes.
    let archive = rows(&server.call("memory_list", args(json!({"path": "specs/archive", "format": "json"}))).await?)?;
    let first = archive
        .iter()
        .map(|r| text_field(r, "key"))
        .find(|k| k.ends_with(".md"))
        .ok_or("empty archive")?;
    server.call("memory_read", args(json!({"key": first, "tier": "l0"}))).await?;
    let refused = server
        .call("memory_write", args(json!({"key": first, "body": "# x\n", "reason": "test"})))
        .await;
    assert!(refused.is_err());

    // Nothing but the edited files and the log changed.
    let after = snapshot(&c.dir);
    let changed: BTreeSet<String> = after
        .iter()
        .filter(|(path, version)| before.get(*path) != Some(*version))
        .map(|(path, _)| path.display().to_string())
        .chain(before.keys().filter(|p| !after.contains_key(*p)).map(|p| p.display().to_string()))
        .collect();
    let expected: BTreeSet<String> = [issue.to_string(), reference.to_string()].into();
    let unexpected: Vec<&String> = changed
        .iter()
        .filter(|p| !expected.contains(*p) && !p.starts_with("memory/_log/"))
        .collect();
    assert!(unexpected.is_empty(), "unexpected changes: {unexpected:?}");
    Ok(())
}

#[tokio::test]
async fn read_version_matches_commit_version() -> Res {
    let c = copy("versions")?;
    let server = start_server(&c).await?;
    let mut checked = 0;
    let mut pending = vec![c.dir.join("specs")];
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder)?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let bytes = std::fs::read(&path)?;
            if std::str::from_utf8(&bytes).is_err() {
                continue;
            }
            let key = path.strip_prefix(&c.dir)?.display().to_string();
            let output = server
                .call("memory_read", args(json!({"key": key, "tier": "l0"})))
                .await
                .map_err(|e| format!("{key}: {e}"))?;
            assert_eq!(version_note(&output), Version::from_content(&bytes).to_string(), "{key}");
            checked += 1;
        }
    }
    assert!(checked > 1000, "only {checked} documents read");
    Ok(())
}

/// The category of a `docs_index.py --check` error, or `None` for a rule `check` declares
/// unsupported.
fn oracle_category(message: &str) -> Option<&'static str> {
    let table: [(&str, &str); 25] = [
        ("is stale", ""),
        ("is missing — run", ""),
        ("no front-matter", "front_matter"),
        ("duplicate id", "duplicate_id"),
        ("does not match filename", "id"),
        ("unknown area", "area"),
        ("has gh_pr and status", "status"),
        ("not in §4.3", "status"),
        ("not in §5.1.1", "readiness"),
        ("not in §5.1", "status"),
        ("priority '", "priority"),
        ("complexity '", "complexity"),
        ("duplicate_of '", "duplicate_of"),
        ("requires a phase", "phase"),
        ("must not carry a phase", "phase"),
        ("unknown phase", "phase"),
        ("(§5.1.1)", "readiness"),
        ("superseded_by '", "superseded_by"),
        ("design '", "design"),
        ("(§5)", "compact_form"),
        ("form '", "compact_form"),
        ("(§5.2.1)", "scenarios"),
        ("audience '", "audience"),
        ("no reviewed:", "reviewed"),
        ("(§9.5)", "history"),
    ];
    if message.contains("dead link") {
        return Some("dead_links");
    }
    if message.contains("source issue or feature") {
        return Some("readiness");
    }
    table
        .iter()
        .find(|(pattern, _)| message.contains(pattern))
        .and_then(|(_, category)| if category.is_empty() { None } else { Some(*category) })
}

/// `(file, category)` of each `docs_index.py --check` error, dropping what `check` does not cover:
/// the generated indexes, links in `specs/README.md` and `specs/index.md` (not memories), and
/// links leaving the mounted folders (source code).
fn oracle(dir: &Path) -> Result<Option<BTreeSet<(String, String)>>, Box<dyn std::error::Error>> {
    let output = match Command::new("python3").arg(dir.join("scripts/docs_index.py")).arg("--check").output() {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Traceback") {
        return Err(format!("docs_index.py failed: {stderr}").into());
    }
    let mut out = BTreeSet::new();
    // The script writes errors to stderr and warnings to stdout.
    for line in stderr.lines() {
        let Some(error) = line.strip_prefix("error: ") else { continue };
        let (file, message) = error.split_once(": ").unwrap_or(("", error));
        let Some(category) = oracle_category(message) else { continue };
        if file == "specs/README.md" || file == "specs/index.md" {
            continue;
        }
        if category == "dead_links" {
            let target = message.trim_start_matches("dead link ").trim_end_matches(" (§8.4)");
            let target = target.split('#').next().unwrap_or("");
            let resolved = normalize(&Path::new(file).parent().unwrap_or(Path::new("")).join(target));
            if !(resolved.starts_with("specs") || resolved.starts_with(".claude/skills")) {
                continue;
            }
        }
        out.insert((file.to_string(), category.to_string()));
    }
    Ok(Some(out))
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The same pairs from `memory_check`, errors only. A field rule names its field first in the
/// message.
async fn checked(server: &McpServer<AgentEnvironment>) -> Result<BTreeSet<(String, String)>, Box<dyn std::error::Error>> {
    let output = server.call("memory_check", args(json!({"path": "specs", "format": "json"}))).await?;
    let mut out = BTreeSet::new();
    for row in rows(&output)? {
        if text_field(&row, "severity") != "error" {
            continue;
        }
        let rule = text_field(&row, "rule");
        let message = text_field(&row, "message");
        let category = match rule.as_str() {
            "required" => message
                .trim_start_matches("required field '")
                .split('\'')
                .next()
                .unwrap_or("")
                .to_string(),
            "enum" | "list" | "date" | "link" | "reciprocal" => message
                .split([':', ' '])
                .next()
                .unwrap_or("")
                .to_string(),
            "id" => "id".to_string(),
            _ => rule,
        };
        out.insert((text_field(&row, "key"), category));
    }
    Ok(out)
}

/// One violation of each rule `check` shares with the script.
fn seed(dir: &Path) -> std::io::Result<()> {
    let issues = dir.join("specs/issues");
    std::fs::write(issues.join("SEED-NO-FRONT-MATTER.md"), "# Seed\n\nNo front-matter.\n")?;
    std::fs::write(
        issues.join("SEED-FIELDS.md"),
        "---\nid: SEED-OTHER\nkind: issue\ntitle: Seeded field violations\nstatus: open\npriority: P9\n\
         complexity: XXL\narea: [nowhere]\ndesign: no-such-design\nduplicate_of: NO-SUCH-ISSUE\n\
         created: 2026-10-19\n---\n# Seeded\n\n## Problem\n\nSee [nothing](NO-SUCH-FILE.md).\n",
    )?;
    let design = |slug: &str, front: &str, body: &str| -> std::io::Result<()> {
        let folder = dir.join("specs/design").join(slug);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("DESIGN.md"), format!("---\n{front}---\n# {slug}\n\n{body}"))
    };
    design("seed-no-phase", "status: draft\n", "")?;
    design("seed-pr", "status: draft\nphase: high-level\ngh_pr: [1]\n", "")?;
    design("seed-bad-values", "status: complete\nphase: sideways\nreadiness: maybe\n", "")?;
    design("seed-superseded", "status: superseded\nsuperseded_by: no-such-design\n", "")?;
    design("seed-form", "status: abandoned\nform: tiny\n", "")?;
    design("seed-compact", "status: abandoned\nform: compact\n", "No phases.\n")?;
    design("seed-readiness", "status: abandoned\nreadiness: ready\nissues: [NO-SUCH-ISSUE]\nautofix: sometimes\n", "")?;
    design(
        "seed-scenarios",
        "status: abandoned\nform: compact\n",
        "## Phase 1\n\n- **AC-1** Without its clauses\n\n## Phase 3\n\nCovers AC-2.\n",
    )?;
    let folder = dir.join("specs/design/seed-links");
    std::fs::create_dir_all(&folder)?;
    std::fs::write(folder.join("DESIGN.md"), "---\nstatus: abandoned\n---\n# Links\n")?;
    std::fs::write(folder.join("phase1-high-level-design.md"), "# P1\n\n[gone](../no-such/DESIGN.md)\n")?;
    std::fs::write(
        dir.join("specs/reference/SEED_REFERENCE.md"),
        "---\ntitle: Seed reference\n---\n# Seed reference\n\nNo audience, no review, no history.\n",
    )?;
    std::fs::write(
        dir.join("specs/guides/SEED_GUIDE.md"),
        "---\ntitle: Seed guide\naudience: internal\nreviewed: 2026-10-19\n---\n# Seed guide\n\n## History\n\n| 2026-01-01 | Old |\n",
    )?;
    Ok(())
}

#[tokio::test]
async fn check_matches_docs_index_oracle() -> Res {
    let c = copy("oracle")?;
    let Some(expected) = oracle(&c.dir)? else {
        eprintln!("python3 is not available; the oracle comparison is skipped");
        return Ok(());
    };
    let unseeded = start_server(&c).await?;
    let found = checked(&unseeded).await?;
    assert_eq!(found, expected, "unseeded copy");

    let report = unseeded.call("memory_check", args(json!({"path": "specs"}))).await?;
    let notes = report.notes.join("\n");
    assert!(notes.contains("index.csv is stale"), "{notes}");

    seed(&c.dir)?;
    let expected = oracle(&c.dir)?.ok_or("python3 vanished")?;
    let categories: BTreeSet<&String> = expected.iter().map(|(_, c)| c).collect();
    assert!(categories.len() >= 15, "seeds cover only {categories:?}");
    let reseeded = start_server(&c).await?;
    let found = checked(&reseeded).await?;
    let missing: Vec<_> = expected.difference(&found).collect();
    let extra: Vec<_> = found.difference(&expected).collect();
    assert!(missing.is_empty() && extra.is_empty(), "missing {missing:?}\nextra {extra:?}");
    Ok(())
}

#[tokio::test]
async fn storage_none_listing_cost() -> Res {
    let c = copy("cost")?;
    let server = start_server(&c).await?;
    let start = Instant::now();
    let output = server
        .call("memory_list", args(json!({"path": "specs/issues", "limit": 1000, "format": "json"})))
        .await?;
    let elapsed = start.elapsed();
    let count = rows(&output)?.len();
    eprintln!("memory_list over {count} entries of specs/issues: {elapsed:?}");
    assert!(elapsed.as_secs_f64() < 2.0, "{elapsed:?}");
    Ok(())
}
