//! Challenge bundles: a folder or zip holding one or more `challenge.yml` files plus their
//! files. The YAML follows ctfcli's format (name, category, description, value, flags,
//! files, connection_info, state) so existing CTFd challenge repos import as they are.
//! Extensions: `difficulty`, `type: file|static|instance`.
use crate::helper::storage;
use crate::models::chall;
use crate::models::manage::{self, Fields};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct Spec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, alias = "points")]
    pub value: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub difficulty: String,
    /// ctfcli's `type` is standard/dynamic; ours is file/static/instance. Anything else = file.
    #[serde(default, rename = "type", skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub connection_info: String,
    #[serde(default)]
    pub flags: Vec<FlagSpec>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FlagSpec {
    Plain(String),
    Full {
        #[serde(rename = "type", default)]
        kind: String,
        content: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        data: String,
    },
}

impl Spec {
    /// Maps to editable fields. Only the first flag is used.
    pub fn fields(&self) -> Fields {
        let (flag_type, flag) = match self.flags.first() {
            Some(FlagSpec::Plain(s)) => ("static", s.clone()),
            Some(FlagSpec::Full {
                kind,
                content,
                data,
            }) => {
                let t = match (kind.as_str(), data.as_str()) {
                    ("regex", _) => "regex",
                    ("per_instance", _) => "per_instance",
                    (_, "case_insensitive") => "case_insensitive",
                    _ => "static",
                };
                (t, content.clone())
            }
            None => ("static", String::new()),
        };
        let ctype = match self.kind.as_str() {
            "static" | "instance" => self.kind.clone(),
            _ if !self.connection_info.is_empty() => "static".into(),
            _ => "file".into(),
        };
        Fields {
            name: self.name.clone(),
            description: self.description.clone(),
            category: self.category.clone(),
            difficulty: self.difficulty.clone(),
            points: if self.value == 0 { 100 } else { self.value },
            ctype,
            static_url: self.connection_info.clone(),
            flag_type: flag_type.into(),
            flag,
        }
    }
}

/// Where bundle files come from.
pub enum Source {
    Dir(PathBuf),
    Zip(zip::ZipArchive<std::fs::File>),
}

const MAX_ENTRY: u64 = 512 * 1024 * 1024;

/// Rejects absolute paths and `..` so a bundle cannot read outside itself.
fn safe_rel(p: &str) -> Option<PathBuf> {
    let path = Path::new(p);
    path.components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        .then(|| path.to_path_buf())
}

impl Source {
    /// Paths (relative, '/'-separated) of every challenge.yml in the bundle.
    pub fn specs(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        match self {
            Source::Dir(root) => walk(root, root, &mut out),
            Source::Zip(z) => {
                for i in 0..z.len() {
                    if let Ok(f) = z.by_index(i) {
                        let n = f.name().to_string();
                        if n.ends_with("challenge.yml") || n.ends_with("challenge.yaml") {
                            out.push(n);
                        }
                    }
                }
            }
        }
        out.sort();
        out
    }

    pub fn read(&mut self, rel: &str) -> Result<Vec<u8>, String> {
        let p = safe_rel(rel).ok_or_else(|| format!("{rel}: path not allowed"))?;
        let mut buf = Vec::new();
        match self {
            Source::Dir(root) => {
                let full = root.join(&p);
                std::fs::File::open(&full)
                    .and_then(|f| f.take(MAX_ENTRY).read_to_end(&mut buf))
                    .map_err(|e| format!("{rel}: {e}"))?;
            }
            Source::Zip(z) => {
                let name = p.to_string_lossy().replace('\\', "/");
                let f = z
                    .by_name(&name)
                    .map_err(|_| format!("{rel}: missing from the archive"))?;
                f.take(MAX_ENTRY)
                    .read_to_end(&mut buf)
                    .map_err(|e| format!("{rel}: {e}"))?;
            }
        }
        Ok(buf)
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else if matches!(
            p.file_name().and_then(|n| n.to_str()),
            Some("challenge.yml" | "challenge.yaml")
        ) && let Ok(rel) = p.strip_prefix(root)
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Imported {
    pub slug: String,
    pub uuid: String,
    pub created: bool,
    pub files: usize,
}

/// Imports every challenge of a bundle. Existing challenges (same slug) are updated in
/// place, keeping their solves; their files are replaced. `can_edit` decides whether the
/// importer may overwrite an existing challenge.
pub async fn import(
    tenant: &str,
    author: &str,
    src: &mut Source,
    publish: bool,
    can_edit: impl AsyncFn(&str) -> bool,
) -> Result<Vec<Imported>, String> {
    let mut done = Vec::new();
    for yml_path in src.specs() {
        let raw = src.read(&yml_path)?;
        let spec: Spec = serde_yaml::from_slice(&raw).map_err(|e| format!("{yml_path}: {e}"))?;
        let fields = spec
            .fields()
            .validate()
            .map_err(|e| format!("{yml_path}: {e}"))?;
        let base = yml_path
            .rsplit_once('/')
            .map(|(d, _)| format!("{d}/"))
            .unwrap_or_default();

        // read every file before touching the database, so a bad bundle changes nothing
        let mut blobs = Vec::new();
        for f in &spec.files {
            let data = src.read(&format!("{base}{f}"))?;
            blobs.push((storage::clean_filename(f), data));
        }

        let slug = manage::slugify(&fields.name);
        let existing = manage::slug_owner(tenant, &slug)
            .await
            .map_err(|e| e.to_string())?;
        let (uuid, created) = match existing {
            Some(uuid) => {
                if !can_edit(&uuid).await {
                    return Err(format!(
                        "{yml_path}: challenge '{slug}' exists and you are not one of its authors"
                    ));
                }
                manage::update(tenant, &uuid, &fields)
                    .await
                    .map_err(|e| e.to_string())?;
                for old in chall::files(&uuid).await.map_err(|e| e.to_string())? {
                    manage::delete_file(&uuid, &old.uuid)
                        .await
                        .map_err(|e| e.to_string())?;
                    storage::remove(tenant, &old.storage_key).await;
                }
                (uuid, false)
            }
            None => (
                manage::create(tenant, author, &fields)
                    .await
                    .map_err(|e| e.to_string())?
                    .0,
                true,
            ),
        };
        let nfiles = blobs.len();
        for (name, data) in blobs {
            let s = storage::store_bytes(tenant, data)
                .await
                .map_err(|e| format!("{name}: {e:?}"))?;
            manage::add_file(&uuid, &name, &s.key, &s.sha256, s.size)
                .await
                .map_err(|e| e.to_string())?;
        }
        let state = match spec.state.as_str() {
            "visible" => Some("published"),
            "hidden" => Some("draft"),
            _ if publish => Some("published"),
            _ => None,
        };
        // publishing needs a flag, like in the editor
        if let Some(st) = state
            && (st == "draft" || !fields.flag.is_empty() || fields.flag_type == "per_instance")
        {
            manage::set_status(tenant, &uuid, st)
                .await
                .map_err(|e| e.to_string())?;
        }
        done.push(Imported {
            slug,
            uuid,
            created,
            files: nfiles,
        });
    }
    if done.is_empty() {
        return Err("No challenge.yml found in this bundle.".into());
    }
    Ok(done)
}

/// Zip with `<slug>/challenge.yml` and `<slug>/dist/<file>`.
pub async fn export(tenant: &str, c: &chall::Chall) -> Result<Vec<u8>, String> {
    let files = chall::files(&c.uuid).await.map_err(|e| e.to_string())?;
    let flag = match c.flag_type.as_str() {
        "static" => FlagSpec::Plain(c.flag.clone()),
        "case_insensitive" => FlagSpec::Full {
            kind: "static".into(),
            content: c.flag.clone(),
            data: "case_insensitive".into(),
        },
        other => FlagSpec::Full {
            kind: other.into(),
            content: c.flag.clone(),
            data: String::new(),
        },
    };
    let spec = Spec {
        name: c.name.clone(),
        author: None,
        category: c.category.clone(),
        description: c.description.clone(),
        value: c.points as i64,
        difficulty: c.difficulty.clone(),
        kind: c.ctype.clone(),
        connection_info: c.static_url.clone(),
        flags: vec![flag],
        files: files
            .iter()
            .map(|f| format!("dist/{}", f.filename))
            .collect(),
        state: if c.status == "published" {
            "visible".into()
        } else {
            "hidden".into()
        },
    };
    let yml = serde_yaml::to_string(&spec).map_err(|e| e.to_string())?;
    let mut blobs = Vec::new();
    for f in &files {
        blobs.push((
            f.filename.clone(),
            storage::read(tenant, &f.storage_key)
                .await
                .map_err(|e| format!("{}: {e}", f.filename))?,
        ));
    }
    let slug = c.slug.clone();
    tokio::task::spawn_blocking(move || {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        z.start_file(format!("{slug}/challenge.yml"), opts)
            .map_err(|e| e.to_string())?;
        z.write_all(yml.as_bytes()).map_err(|e| e.to_string())?;
        for (name, data) in blobs {
            z.start_file(format!("{slug}/dist/{name}"), opts)
                .map_err(|e| e.to_string())?;
            z.write_all(&data).map_err(|e| e.to_string())?;
        }
        z.finish()
            .map(|c| c.into_inner())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctfcli_yaml() {
        let y = r#"
name: "Cookie Monster"
author: alice
category: Web
description: |
  Eat **all** the cookies.
value: 250
type: dynamic
connection_info: https://cookies.example.com
flags:
  - {type: static, content: "LH{om_nom}", data: case_insensitive}
  - LH{ignored}
files:
  - dist/cookies.txt
state: visible
"#;
        let s: Spec = serde_yaml::from_str(y).unwrap();
        let f = s.fields().validate().unwrap();
        assert_eq!(f.name, "Cookie Monster");
        assert_eq!(f.category, "web");
        assert_eq!(f.points, 250);
        assert_eq!(f.ctype, "static");
        assert_eq!(f.flag_type, "case_insensitive");
        assert_eq!(f.flag, "LH{om_nom}");
        assert_eq!(s.files, vec!["dist/cookies.txt"]);
        let plain: Spec = serde_yaml::from_str("name: x\nflags: [\"LH{a}\"]\n").unwrap();
        assert_eq!(plain.fields().flag, "LH{a}");
        assert_eq!(plain.fields().points, 100);
    }

    #[test]
    fn bundle_paths_are_confined() {
        assert!(safe_rel("dist/a.txt").is_some());
        assert!(safe_rel("../secret").is_none());
        assert!(safe_rel("/etc/passwd").is_none());
    }
}
