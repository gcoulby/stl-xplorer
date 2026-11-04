//! Tags applied from path contents. A tag with keywords is put on every file whose path,
//! relative to its library, contains one of them as whole words. Rows written here carry
//! `auto = 1`, so changing a rule only ever adds or removes its own rows, never manual tags.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::types::TagSuggestion;

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    words: Vec<String>,
    /// `ork*` matches orks and orkz as well as ork.
    prefix: bool,
}

/// Lower-case runs of letters and digits.
pub fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Comma or newline separated keywords. Each may be several words.
pub fn parse_keywords(s: &str) -> Vec<Pattern> {
    s.split([',', '\n', ';'])
        .filter_map(|k| {
            let k = k.trim();
            let prefix = k.ends_with('*');
            let words = tokens(k.trim_end_matches('*'));
            (!words.is_empty()).then_some(Pattern { words, prefix })
        })
        .collect()
}

pub fn matches(hay: &[String], p: &Pattern) -> bool {
    let n = p.words.len();
    if n == 0 || hay.len() < n {
        return false;
    }
    hay.windows(n).any(|w| {
        w.iter().zip(&p.words).enumerate().all(|(i, (h, k))| {
            if p.prefix && i == n - 1 {
                h.starts_with(k.as_str())
            } else {
                h == k
            }
        })
    })
}

struct Rule {
    tag_id: i64,
    patterns: Vec<Pattern>,
}

fn load_rules(conn: &Connection, only_tag: Option<i64>) -> AppResult<Vec<Rule>> {
    let mut stmt = conn.prepare(
        "SELECT id, keywords FROM tags
         WHERE keywords IS NOT NULL AND trim(keywords) <> '' AND (?1 IS NULL OR id = ?1)",
    )?;
    let rows = stmt.query_map([only_tag], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (tag_id, kw) = row?;
        let patterns = parse_keywords(&kw);
        if !patterns.is_empty() {
            out.push(Rule { tag_id, patterns });
        }
    }
    Ok(out)
}

/// The text a rule is matched against: everything below the library folder.
const HAYSTACK_SQL: &str = "SELECT m.id,
   CASE WHEN c.kind = 'folder' THEN m.rel_path ELSE c.path || '/' || m.rel_path END,
   l.path
 FROM models m JOIN containers c ON c.id = m.container_id JOIN libraries l ON l.id = m.library_id";

fn haystack(full: &str, library_path: &str) -> Vec<String> {
    tokens(full.strip_prefix(library_path).unwrap_or(full))
}

/// Bring auto tags up to date for some models (or all of them) and some tag (or every tag).
/// Returns the models whose tags changed, so the caller can refresh their search rows.
pub fn apply(
    conn: &Connection,
    model_ids: Option<&[i64]>,
    only_tag: Option<i64>,
) -> AppResult<Vec<i64>> {
    let rules = load_rules(conn, only_tag)?;

    // What the rules want, over the models in scope.
    let mut scope: Vec<i64> = Vec::new();
    let mut want: HashSet<(i64, i64)> = HashSet::new();
    let mut consider = |id: i64, full: String, lib: String| {
        scope.push(id);
        if rules.is_empty() {
            return;
        }
        let hay = haystack(&full, &lib);
        for r in &rules {
            if r.patterns.iter().any(|p| matches(&hay, p)) {
                want.insert((id, r.tag_id));
            }
        }
    };
    match model_ids {
        Some(ids) => {
            let mut stmt = conn.prepare_cached(&format!("{HAYSTACK_SQL} WHERE m.id = ?1"))?;
            for id in ids {
                let mut rows = stmt.query([id])?;
                if let Some(r) = rows.next()? {
                    consider(r.get(0)?, r.get(1)?, r.get(2)?);
                }
            }
        }
        None => {
            let mut stmt = conn.prepare(HAYSTACK_SQL)?;
            let mut rows = stmt.query([])?;
            while let Some(r) = rows.next()? {
                consider(r.get(0)?, r.get(1)?, r.get(2)?);
            }
        }
    }

    // What is there now.
    let mut have: HashSet<(i64, i64)> = HashSet::new();
    {
        let mut stmt = conn.prepare(
            "SELECT model_id, tag_id FROM model_tags WHERE auto = 1 AND (?1 IS NULL OR tag_id = ?1)",
        )?;
        let in_scope: Option<HashSet<i64>> = model_ids.map(|_| scope.iter().copied().collect());
        let rows = stmt.query_map([only_tag], |r| Ok((r.get::<_, i64>(0)?, r.get(1)?)))?;
        for row in rows {
            let pair = row?;
            if in_scope.as_ref().is_none_or(|s| s.contains(&pair.0)) {
                have.insert(pair);
            }
        }
    }

    let mut changed: HashMap<i64, ()> = HashMap::new();
    let mut del = conn.prepare_cached(
        "DELETE FROM model_tags WHERE model_id = ?1 AND tag_id = ?2 AND auto = 1",
    )?;
    for pair in have.difference(&want) {
        if del.execute(params![pair.0, pair.1])? > 0 {
            changed.insert(pair.0, ());
        }
    }
    // A manual row for the same tag wins; leave it as it is.
    let mut ins = conn.prepare_cached(
        "INSERT OR IGNORE INTO model_tags (model_id, tag_id, auto) VALUES (?1, ?2, 1)",
    )?;
    for pair in want.difference(&have) {
        if ins.execute(params![pair.0, pair.1])? > 0 {
            changed.insert(pair.0, ());
        }
    }
    Ok(changed.into_keys().collect())
}

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "stl", "stls", "obj", "3mf", "zip", "rar", "files",
    "file", "part", "parts", "version", "final", "copy", "all", "images", "image", "jpg", "png",
    "pdf", "new", "old", "folder", "untitled", "misc", "other", "others",
];

/// Words that turn up in many folder and archive names, as starting points for tags.
pub fn suggest(conn: &Connection, limit: usize) -> AppResult<Vec<TagSuggestion>> {
    let existing: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT name, COALESCE(keywords, '') FROM tags")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut set = HashSet::new();
        for row in rows {
            let (name, kw) = row?;
            set.extend(tokens(&name));
            set.extend(tokens(&kw));
        }
        set
    };
    let mut counts: HashMap<String, (i64, i64)> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT c.name, c.model_count FROM containers c JOIN libraries l ON l.id = c.library_id
         WHERE c.parent_id IS NOT NULL AND c.missing = 0 AND c.model_count > 0 AND l.enabled = 1",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (name, models) = row?;
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&name);
        let words: HashSet<String> = tokens(stem).into_iter().collect();
        for w in words {
            if w.len() < 3
                || w.chars().all(|c| c.is_ascii_digit())
                || STOP_WORDS.contains(&w.as_str())
                || existing.contains(&w)
            {
                continue;
            }
            let e = counts.entry(w).or_insert((0, 0));
            e.0 += 1;
            e.1 += models;
        }
    }
    let mut out: Vec<TagSuggestion> = counts
        .into_iter()
        .filter(|(_, (c, _))| *c >= 2)
        .map(|(word, (containers, models))| TagSuggestion {
            word,
            containers,
            models,
        })
        .collect();
    out.sort_by(|a, b| {
        b.containers
            .cmp(&a.containers)
            .then(b.models.cmp(&a.models))
            .then(a.word.cmp(&b.word))
    });
    out.truncate(limit);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_matching() {
        let hay = tokens("Cromarty Forge - Ork Scenery 10mm/Orkz_Fence.stl");
        assert!(matches(&hay, &parse_keywords("ork")[0]));
        assert!(matches(&hay, &parse_keywords("orkz fence")[0]));
        assert!(matches(&hay, &parse_keywords("orkz fen*")[0]));
        assert!(matches(&hay, &parse_keywords("scen*")[0]));
        assert!(
            !matches(&hay, &parse_keywords("scen")[0]),
            "whole words only"
        );
        assert!(
            !matches(&hay, &parse_keywords("cromarty ork")[0]),
            "words must be adjacent"
        );
        assert_eq!(parse_keywords(" ork*, space marine ;, ").len(), 2);
    }

    #[test]
    fn rules_add_and_remove_only_their_own_rows() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/lib/orks');
             INSERT INTO containers (id, library_id, parent_id, kind, path, name) VALUES
               (1, 1, NULL, 'folder', '/lib/orks', 'L'),
               (2, 1, 1, 'zip', '/lib/orks/Ork Boyz.zip', 'Ork Boyz.zip');
             INSERT INTO models (id, container_id, library_id, rel_path, name, ext) VALUES
               (1, 2, 1, 'boy.stl', 'boy.stl', 'stl'),
               (2, 1, 1, '/lib/orks/tree.stl', 'tree.stl', 'stl');
             INSERT INTO tags (id, name, keywords) VALUES (1, 'Orks', 'ork*'), (2, 'Trees', NULL);
             INSERT INTO model_tags (model_id, tag_id, auto) VALUES (2, 1, 0);",
        )
        .expect("seed");
        let changed = apply(&conn, None, None).expect("apply");
        // The library folder name does not count, so only the zip's model is tagged.
        assert_eq!(changed, vec![1]);
        let auto: i64 = conn
            .query_row("SELECT COUNT(*) FROM model_tags WHERE auto = 1", [], |r| {
                r.get(0)
            })
            .expect("count");
        assert_eq!(auto, 1);

        conn.execute("UPDATE tags SET keywords = NULL WHERE id = 1", [])
            .expect("clear");
        apply(&conn, None, Some(1)).expect("apply");
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM model_tags", [], |r| r.get(0))
            .expect("count");
        assert_eq!(rows, 1, "the manual tag stays");
    }

    #[test]
    fn suggests_common_words() {
        let conn = crate::db::test_conn();
        conn.execute_batch(
            "INSERT INTO libraries (id, name, path) VALUES (1, 'L', '/l');
             INSERT INTO containers (library_id, parent_id, kind, path, name, model_count) VALUES
               (1, NULL, 'folder', '/l', 'L', 9),
               (1, 1, 'folder', '/l/a', 'Ork Boyz', 3),
               (1, 1, 'zip', '/l/b.zip', 'Ork Trukk.zip', 3),
               (1, 1, 'folder', '/l/c', 'Elf Archers', 3);",
        )
        .expect("seed");
        let s = suggest(&conn, 10).expect("suggest");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].word, "ork");
        assert_eq!(s[0].containers, 2);
    }
}
