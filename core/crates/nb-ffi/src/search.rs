//! Search over notebooks (P1-32): notebook titles, section and page titles, typed text,
//! maths LaTeX and plot expressions. Matching ignores case and accents ("Équation" matches
//! "equation"); every word of the query must appear in the same item.
//!
//! Opening every page of every notebook for each query would be far too slow, so
//! [`SearchIndex`] keeps the extracted texts in memory and in a cache file, and re-reads
//! only the notebooks and pages whose files changed since they were indexed (size and
//! modification time of the SQLite file and its WAL). Pages are read without writing to
//! them (no `PageDoc::open`, which records a version).

use crate::CoreError;
use nb_model::crdt::CrdtPage;
use nb_model::notebook::{CrdtNotebook, NodeKind};
use nb_model::Content;
use nb_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Lower case without accents, one output char per input char where possible
/// (`æ`, `œ`, `ß` become two). Returns the folded text and, for each folded char, the index
/// of the char of `s` it comes from. Combining accents (decomposed text) are dropped.
pub fn fold(s: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(s.len());
    let mut map = Vec::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        if ('\u{300}'..='\u{36f}').contains(&c) {
            continue;
        }
        for l in c.to_lowercase() {
            let base: &str = match l {
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
                'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
                'ď' | 'đ' => "d",
                'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
                'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
                'ĥ' | 'ħ' => "h",
                'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
                'ĵ' => "j",
                'ķ' => "k",
                'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
                'ñ' | 'ń' | 'ņ' | 'ň' => "n",
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
                'ŕ' | 'ŗ' | 'ř' => "r",
                'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
                'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
                'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
                'ŵ' => "w",
                'ý' | 'ÿ' | 'ŷ' => "y",
                'ź' | 'ż' | 'ž' => "z",
                'æ' => "ae",
                'œ' => "oe",
                'ß' => "ss",
                // Typographic apostrophe and non-breaking spaces as typed on keyboards.
                '\u{2019}' => "'",
                '\u{a0}' | '\u{202f}' => " ",
                _ => {
                    out.push(l);
                    map.push(i);
                    continue;
                }
            };
            for b in base.chars() {
                out.push(b);
                map.push(i);
            }
        }
    }
    (out, map)
}

/// Words of a query, folded.
fn terms(query: &str) -> Vec<String> {
    fold(query).0.split_whitespace().map(str::to_string).collect()
}

/// Where the first term occurs in `text` (char range of the original text), if every term
/// occurs in it.
fn find(text: &Folded, terms: &[String]) -> Option<(usize, usize)> {
    if terms.is_empty() || !terms.iter().all(|t| text.folded.contains(t.as_str())) {
        return None;
    }
    let t = &terms[0];
    let byte = text.folded.find(t.as_str())?;
    let start = text.folded[..byte].chars().count();
    let n = t.chars().count();
    let from = *text.map.get(start)?;
    let to = *text.map.get(start + n - 1)? + 1;
    Some((from, to))
}

/// A short excerpt around chars `from..to` of `text`, on one line.
fn snippet(text: &str, from: usize, to: usize) -> (String, String, String) {
    let chars: Vec<char> = text.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect();
    let a = from.saturating_sub(30);
    let b = (to + 60).min(chars.len());
    let mut before: String = chars[a..from].iter().collect();
    if a > 0 {
        before.insert(0, '…');
    }
    let mut after: String = chars[to..b].iter().collect();
    if b < chars.len() {
        after.push('…');
    }
    (before, chars[from..to].iter().collect(), after)
}

#[derive(Clone, Debug, Default)]
struct Folded {
    folded: String,
    map: Vec<usize>,
}

impl Folded {
    fn of(s: &str) -> Self {
        let (folded, map) = fold(s);
        Self { folded, map }
    }
}

/// What a search hit matched.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HitKind {
    NotebookTitle,
    SectionTitle,
    PageTitle,
    Text,
    Math,
    Plot,
}

/// One search result. `before` + `matched` + `after` is a one-line excerpt with the match
/// in the middle (to show it highlighted).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub kind: HitKind,
    /// Notebook directory name (under the root).
    pub notebook_dir: String,
    pub notebook_title: String,
    /// Page to open (first page of the section for a section title; `None` for a notebook title).
    pub page_id: Option<String>,
    pub page_title: String,
    /// 1-based position of the page in the notebook (0 for a notebook title).
    pub page_number: u32,
    /// Block to highlight on the page (text, maths, plot), with its bounds in page units.
    pub block_id: Option<String>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub before: String,
    pub matched: String,
    pub after: String,
}

/// Size and modification time of a SQLite file and of its WAL: changes whenever it is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp(u64, u128, u64, u128);

fn stamp(db: &Path) -> Option<Stamp> {
    let one = |p: &Path| {
        std::fs::metadata(p).ok().map(|m| {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            (m.len(), t)
        })
    };
    let (a, b) = one(db)?;
    let mut wal = db.as_os_str().to_owned();
    wal.push("-wal");
    let (c, d) = one(Path::new(&wal)).unwrap_or((0, 0));
    Some(Stamp(a, b, c, d))
}

/// Updates stored in a SQLite store, without creating it if it is missing.
fn read_chunks(db: &Path) -> Option<Vec<Vec<u8>>> {
    if !db.is_file() {
        return None;
    }
    let loaded = Store::open(db).ok()?.load().ok()?;
    let mut chunks: Vec<Vec<u8>> = loaded.snapshot.into_iter().collect();
    chunks.extend(loaded.updates);
    Some(chunks)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Block {
    id: String,
    kind: HitKind,
    text: String,
    rect: [f32; 4],
    #[serde(skip)]
    folded: Option<Folded>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PageEntry {
    stamp: Stamp,
    blocks: Vec<Block>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OutlineRow {
    kind: HitKind, // SectionTitle or PageTitle
    title: String,
    /// The page, or the first page of the section.
    page_id: Option<String>,
    #[serde(skip)]
    folded: Option<Folded>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct NotebookEntry {
    stamp: Stamp,
    title: String,
    outline: Vec<OutlineRow>,
    pages: HashMap<String, PageEntry>,
    #[serde(skip)]
    folded: Option<Folded>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    version: u32,
    notebooks: HashMap<String, NotebookEntry>,
}

const INDEX_VERSION: u32 = 1;

fn page_blocks(path: &Path) -> Vec<Block> {
    let Some(chunks) = read_chunks(path) else { return vec![] };
    let refs: Vec<&[u8]> = chunks.iter().map(Vec::as_slice).collect();
    let Ok(page) = CrdtPage::load(&refs, 1) else {
        return vec![];
    };
    let r = |r: &nb_model::Rect| [r.x as f32, r.y as f32, r.w as f32, r.h as f32];
    page.page()
        .items
        .iter()
        .filter_map(|it| {
            let (kind, text, rect) = match &it.content {
                Content::Text { rect, text, .. } => (HitKind::Text, text.clone(), r(rect)),
                Content::Math { rect, latex, .. } => (HitKind::Math, latex.clone(), r(rect)),
                Content::Plot { rect, exprs, .. } => (HitKind::Plot, exprs.join("  ;  "), r(rect)),
                _ => return None,
            };
            (!text.trim().is_empty()).then(|| Block {
                id: it.id.clone(),
                kind,
                text,
                rect,
                folded: None,
            })
        })
        .collect()
}

impl Index {
    /// Bring the entry of notebook `dir` up to date. Returns whether anything was re-read.
    fn refresh(&mut self, root: &Path, dir: &str) -> bool {
        let path = root.join(dir);
        let Some(nb_stamp) = stamp(&path.join("notebook.db")) else {
            return self.notebooks.remove(dir).is_some();
        };
        let mut changed = false;
        let e = self.notebooks.entry(dir.to_string()).or_default();
        if e.stamp != nb_stamp || e.outline.is_empty() && e.title.is_empty() {
            let Some(chunks) = read_chunks(&path.join("notebook.db")) else {
                return false;
            };
            let refs: Vec<&[u8]> = chunks.iter().map(Vec::as_slice).collect();
            let Ok(nb) = CrdtNotebook::load(&refs, 1) else {
                return false;
            };
            e.title = nb.title();
            e.folded = None;
            let outline = nb.outline();
            e.outline = outline
                .iter()
                .enumerate()
                .map(|(i, n)| OutlineRow {
                    kind: if n.kind == NodeKind::Section {
                        HitKind::SectionTitle
                    } else {
                        HitKind::PageTitle
                    },
                    title: n.title.clone(),
                    page_id: n.page_id.clone().or_else(|| {
                        outline[i + 1..]
                            .iter()
                            .take_while(|m| m.depth > n.depth)
                            .find_map(|m| m.page_id.clone())
                    }),
                    folded: None,
                })
                .collect();
            e.stamp = nb_stamp;
            changed = true;
        }
        // Pages: re-read changed ones, forget removed ones.
        let ids: Vec<String> = e
            .outline
            .iter()
            .filter(|r| r.kind == HitKind::PageTitle)
            .filter_map(|r| r.page_id.clone())
            .collect();
        let before = e.pages.len();
        e.pages.retain(|id, _| ids.contains(id));
        changed |= e.pages.len() != before;
        for id in ids {
            let file = path.join(format!("{id}.kpage"));
            let s = stamp(&file).unwrap_or_default();
            if e.pages.get(&id).is_some_and(|p| p.stamp == s) {
                continue;
            }
            let blocks = page_blocks(&file);
            // Stamp after reading: closing the last connection may checkpoint the WAL.
            let s = stamp(&file).unwrap_or_default();
            e.pages.insert(id, PageEntry { stamp: s, blocks });
            changed = true;
        }
        changed
    }
}

struct State {
    root: PathBuf,
    file: PathBuf,
    index: Index,
}

/// Search index over the notebooks under a root directory, cached in a file.
#[derive(uniffi::Object)]
pub struct SearchIndex {
    state: Mutex<State>,
}

#[uniffi::export]
impl SearchIndex {
    /// Index of the notebooks under `root`, cached in `index_file` (a cache: it is rebuilt if
    /// missing or unreadable).
    #[uniffi::constructor]
    pub fn open(root: String, index_file: String) -> std::sync::Arc<Self> {
        let file = PathBuf::from(index_file);
        let index = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Index>(&b).ok())
            .filter(|i| i.version == INDEX_VERSION)
            .unwrap_or(Index {
                version: INDEX_VERSION,
                notebooks: HashMap::new(),
            });
        std::sync::Arc::new(Self {
            state: Mutex::new(State {
                root: PathBuf::from(root),
                file,
                index,
            }),
        })
    }

    /// Search notebooks `dirs` (directory names, in the order results should come: e.g. most
    /// recent first, or only the open notebook) for `query`. At most `limit` hits, in
    /// reading order within each notebook.
    pub fn search(&self, query: String, dirs: Vec<String>, limit: u32) -> Result<Vec<SearchHit>, CoreError> {
        let terms = terms(&query);
        // Maths queries (E2.13) such as "x^2 + ?" also match formulas by structure.
        let maths = nb_math::mathsearch::MathQuery::parse(&query);
        let mut st = self.state.lock().unwrap();
        let root = st.root.clone();
        let mut changed = false;
        for d in &dirs {
            changed |= st.index.refresh(&root, d);
        }
        if changed {
            let json = serde_json::to_vec(&st.index).map_err(|e| CoreError::Format { detail: e.to_string() })?;
            let tmp = st.file.with_extension("tmp");
            std::fs::write(&tmp, json)
                .and_then(|_| std::fs::rename(&tmp, &st.file))
                .map_err(|e| CoreError::Io { detail: e.to_string() })?;
        }
        let mut hits = vec![];
        if terms.is_empty() && maths.is_none() {
            return Ok(hits);
        }
        let limit = limit as usize;
        for d in &dirs {
            let Some(e) = st.index.notebooks.get_mut(d) else {
                continue;
            };
            let hit = |kind, page_id: Option<String>, page_title: &str, n, text: &str, (f, t)| {
                let (before, matched, after) = snippet(text, f, t);
                SearchHit {
                    kind,
                    notebook_dir: d.clone(),
                    notebook_title: String::new(),
                    page_id,
                    page_title: page_title.to_string(),
                    page_number: n,
                    block_id: None,
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                    before,
                    matched,
                    after,
                }
            };
            let mut found = vec![];
            let title = e.title.clone();
            if let Some(m) = find(e.folded.get_or_insert_with(|| Folded::of(&title)), &terms) {
                found.push(hit(HitKind::NotebookTitle, None, "", 0, &title, m));
            }
            let mut number = 0;
            for row in e.outline.iter_mut() {
                if row.kind == HitKind::PageTitle {
                    number += 1;
                }
                let t = row.title.clone();
                if let Some(m) = find(row.folded.get_or_insert_with(|| Folded::of(&t)), &terms) {
                    let n = if row.kind == HitKind::PageTitle {
                        number
                    } else {
                        number + 1
                    };
                    found.push(hit(row.kind, row.page_id.clone(), &t, n, &t, m));
                }
                if row.kind != HitKind::PageTitle {
                    continue;
                }
                let Some(page) = row.page_id.as_ref().and_then(|id| e.pages.get_mut(id)) else {
                    continue;
                };
                for b in page.blocks.iter_mut() {
                    let text = &b.text;
                    let by_text = if terms.is_empty() {
                        None
                    } else {
                        find(b.folded.get_or_insert_with(|| Folded::of(text)), &terms)
                    };
                    // Structural match: the whole formula is the snippet.
                    let by_maths = match (&maths, b.kind) {
                        (Some(q), HitKind::Math) if q.matches(&b.text, true) => Some((0, b.text.chars().count())),
                        (Some(q), HitKind::Plot) if b.text.lines().any(|l| q.matches(l, false)) => {
                            Some((0, b.text.chars().count()))
                        }
                        _ => None,
                    };
                    if let Some(m) = by_text.or(by_maths) {
                        let mut h = hit(b.kind, row.page_id.clone(), &t, number, &b.text, m);
                        h.block_id = Some(b.id.clone());
                        [h.x, h.y, h.width, h.height] = b.rect;
                        found.push(h);
                    }
                }
                if found.len() >= limit {
                    break;
                }
            }
            for mut h in found {
                if hits.len() >= limit {
                    return Ok(hits);
                }
                h.notebook_title = title.clone();
                hits.push(h);
            }
        }
        Ok(hits)
    }
}

/// Pages to export from a page range typed by the user, e.g. `1-3, 5, 8-` (P1-33): 0-based
/// page indices in increasing order without duplicates, for a notebook of `count` pages.
/// An empty text means every page. `None` if the text is not a valid range, or selects no
/// page.
#[uniffi::export]
pub fn parse_page_range(text: String, count: u32) -> Option<Vec<u32>> {
    let text = text.trim();
    if text.is_empty() {
        return (count > 0).then(|| (0..count).collect());
    }
    let mut pages = std::collections::BTreeSet::new();
    for part in text.split([',', ';']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let num = |s: &str| -> Option<u32> {
            let s = s.trim();
            if s.is_empty() {
                return None;
            }
            s.parse::<u32>().ok().filter(|&n| n >= 1)
        };
        let (a, b) = match part.split_once(['-', '–']) {
            Some((a, b)) => (
                if a.trim().is_empty() { Some(1) } else { num(a) }?,
                if b.trim().is_empty() { Some(count) } else { num(b) }?,
            ),
            None => {
                let n = num(part)?;
                (n, n)
            }
        };
        if a > b || b > count {
            return None;
        }
        pages.extend(a - 1..b);
    }
    (!pages.is_empty()).then(|| pages.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_model::{Item, Page, Rect};

    #[test]
    fn folding_ignores_case_and_accents() {
        assert_eq!(fold("Équation DÉRIVÉE").0, "equation derivee");
        assert_eq!(fold("Cœur, garçon, ñandú, Straße").0, "coeur, garcon, nandu, strasse");
        assert_eq!(fold("Fanazavana momba ny fôrmiola").0, "fanazavana momba ny formiola");
        // Decomposed accents (e + combining acute) fold the same way.
        assert_eq!(fold("e\u{301}te\u{301}").0, "ete");
        // The map points back to original chars, even across expansions.
        let (f, map) = fold("Œuf é");
        assert_eq!(f, "oeuf e");
        assert_eq!(map, [0, 0, 1, 2, 3, 4]);
        let t = Folded::of("La dérivée de x² est 2x");
        assert_eq!(find(&t, &terms("DERIVEE")), Some((3, 10)));
        assert_eq!(
            find(&t, &terms("2x dérivée")),
            Some((21, 23)),
            "highlights the first word"
        );
        assert_eq!(find(&t, &terms("integrale")), None);
        assert_eq!(find(&t, &terms("  ")), None);
        let (b, m, a) = snippet("ligne 1\nla Dérivée ici", 11, 18);
        assert_eq!((b.as_str(), m.as_str(), a.as_str()), ("ligne 1 la ", "Dérivée", " ici"));
    }

    #[test]
    fn page_ranges() {
        let p = |s: &str, n| parse_page_range(s.into(), n);
        assert_eq!(p("", 3), Some(vec![0, 1, 2]));
        assert_eq!(p("2", 3), Some(vec![1]));
        assert_eq!(p("1-2, 5; 4", 6), Some(vec![0, 1, 3, 4]));
        assert_eq!(p("3-", 5), Some(vec![2, 3, 4]));
        assert_eq!(p("-2", 5), Some(vec![0, 1]));
        assert_eq!(p("2–3", 5), Some(vec![1, 2]), "en dash");
        assert_eq!(p("2-2,2", 5), Some(vec![1]));
        for bad in ["0", "4-2", "7", "1-9", "a", "1-b", "1,,x"] {
            assert_eq!(p(bad, 6), None, "{bad}");
        }
        assert_eq!(p("", 0), None);
    }

    fn write_page(path: &Path, page: &Page) {
        let doc = CrdtPage::from_page(page, 7);
        Store::open(path).unwrap().compact(&doc.snapshot()).unwrap();
    }

    fn text(id: &str, s: &str, y: f64) -> Item {
        Item {
            id: id.into(),
            content: Content::Text {
                rect: Rect {
                    x: 10.0,
                    y,
                    w: 300.0,
                    h: 30.0,
                },
                text: s.into(),
                spans: vec![],
                size: 20.0,
                color: 0,
            },
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kahie-search-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn finds_titles_text_maths_and_plots_and_follows_edits() {
        let root = tmp("basic");
        let nb = crate::NotebookDoc::open(root.join("nb-1").to_string_lossy().into(), "Mathématiques".into()).unwrap();
        let p1 = nb.page_ids()[0].clone();
        let sec = nb.add_section("Fonctions dérivées".into());
        let p2 = nb.add_page(None);
        let node = nb
            .outline()
            .into_iter()
            .find(|e| e.page_id.as_deref() == Some(&p2))
            .unwrap()
            .node;
        nb.move_node(node.clone(), Some(sec), 0);
        nb.rename(node, "Exercices".into());
        let page = crate::PageDoc::open(nb.page_path(p1.clone())).unwrap();
        let t = page.add_text(10.0, 40.0, 300.0, 30.0, "Théorème de Pythagore".into(), 20.0, 0);
        page.add_math(10.0, 100.0, 80.0, 30.0, "\\frac{a}{b}".into(), 24.0, 0);
        page.add_plot(10.0, 200.0, 300.0, 200.0, vec!["sin(x)".into()], -5.0, 5.0, -2.0, 2.0)
            .unwrap();
        let index_file = root.join("index.json");
        let idx = SearchIndex::open(root.to_string_lossy().into(), index_file.to_string_lossy().into());
        let dirs = vec!["nb-1".to_string()];
        let s = |q: &str| idx.search(q.into(), dirs.clone(), 50).unwrap();
        // Maths search by structure (E2.13).
        assert_eq!(
            s("\\frac{?}{b}").iter().map(|h| h.kind).collect::<Vec<_>>(),
            [HitKind::Math]
        );
        assert_eq!(s("sin(?)").iter().map(|h| h.kind).collect::<Vec<_>>(), [HitKind::Plot]);
        assert!(s("x^2 + ?").is_empty());
        let h = s("pythagore theoreme");
        assert_eq!(h.len(), 1);
        assert_eq!(
            (h[0].kind, h[0].block_id.as_deref(), h[0].page_number),
            (HitKind::Text, Some(t.as_str()), 1)
        );
        assert_eq!(
            (h[0].matched.as_str(), h[0].after.as_str(), h[0].y),
            ("Pythagore", "", 40.0)
        );
        assert_eq!(s("FRAC")[0].kind, HitKind::Math);
        assert_eq!(s("sin")[0].kind, HitKind::Plot);
        assert_eq!(s("mathematiques")[0].kind, HitKind::NotebookTitle);
        let h = s("derivees");
        assert_eq!(
            (h[0].kind, h[0].page_id.as_deref(), h[0].page_number),
            (HitKind::SectionTitle, Some(p2.as_str()), 2)
        );
        let h = s("exercice");
        assert_eq!(
            (h[0].kind, h[0].page_number, h[0].page_title.as_str()),
            (HitKind::PageTitle, 2, "Exercices")
        );
        assert!(s("introuvable").is_empty() && s("").is_empty());
        // An edit of the open page (still open, in its WAL) is found by the next search.
        page.set_text(t.clone(), "Théorème de Thalès".into(), 30.0);
        assert!(s("pythagore").is_empty());
        assert_eq!(s("thales").len(), 1);
        // The cache file serves a new index; a deleted page disappears from results.
        drop(idx);
        let idx = SearchIndex::open(root.to_string_lossy().into(), index_file.to_string_lossy().into());
        assert_eq!(idx.search("thales".into(), dirs.clone(), 50).unwrap().len(), 1);
        let n1 = nb
            .outline()
            .into_iter()
            .find(|e| e.page_id.as_deref() == Some(&p1))
            .unwrap()
            .node;
        nb.delete_node(n1);
        assert!(idx.search("thales".into(), dirs.clone(), 50).unwrap().is_empty());
        assert_eq!(idx.search("e".into(), dirs, 2).unwrap().len(), 2, "limit");
        std::fs::remove_dir_all(root).ok();
    }

    /// P1-32 budget: a search across 100 notebooks takes ≤ 300 ms once they are indexed
    /// (the index is kept up to date as pages change, so this is the cost of every query).
    #[test]
    fn search_across_100_notebooks_within_300_ms() {
        let root = tmp("perf");
        let words = [
            "fonction",
            "dérivée",
            "intégrale",
            "vecteur",
            "probabilité",
            "suite",
            "limite",
            "équation",
        ];
        let mut dirs = vec![];
        for n in 0..100 {
            let dir = format!("nb-{n:04}");
            let path = root.join(&dir);
            std::fs::create_dir_all(&path).unwrap();
            let mut nb = CrdtNotebook::new(&dir, &format!("Classe {n}"), 7);
            for p in 0..10 {
                let id = format!("p{p}");
                nb.add_page(None, None, &id, &format!("Leçon {p}"));
                let items = (0..6)
                    .map(|k| {
                        let w = words[(n + p + k) % words.len()];
                        text(
                            &format!("t{k}"),
                            &format!("Cours {n}.{p}.{k} : la {w} et ses propriétés, exemples et exercices corrigés."),
                            40.0 * k as f64,
                        )
                    })
                    .collect();
                let page = Page {
                    id: id.clone(),
                    template: "lined".into(),
                    size: "a4".into(),
                    items,
                    groups: Default::default(),
                    pdf: None,
                };
                write_page(&path.join(format!("{id}.kpage")), &page);
            }
            Store::open(path.join("notebook.db"))
                .unwrap()
                .compact(&nb.snapshot())
                .unwrap();
            dirs.push(dir);
        }
        let file = root.join("index.json").to_string_lossy().to_string();
        let idx = SearchIndex::open(root.to_string_lossy().into(), file.clone());
        let t0 = std::time::Instant::now();
        let first = idx.search("integrale".into(), dirs.clone(), 10_000).unwrap();
        let build = t0.elapsed();
        let expected = (0..100usize)
            .flat_map(|n| (0..10usize).flat_map(move |p| (0..6usize).map(move |k| (n + p + k) % 8)))
            .filter(|&w| w == 2)
            .count();
        assert_eq!(first.len(), expected);
        let mut worst = std::time::Duration::ZERO;
        for q in ["probabilite exercices", "Équation", "zzz", "cours 42.3"] {
            let t = std::time::Instant::now();
            idx.search(q.into(), dirs.clone(), 10_000).unwrap();
            worst = worst.max(t.elapsed());
        }
        // A fresh process: the index comes from its cache file.
        let t = std::time::Instant::now();
        let again = SearchIndex::open(root.to_string_lossy().into(), file);
        assert_eq!(
            again.search("integrale".into(), dirs.clone(), 10_000).unwrap().len(),
            first.len()
        );
        let reopened = t.elapsed();
        eprintln!("search over 100 notebooks × 10 pages: first (indexing) {build:?}, then worst {worst:?}, from cache file {reopened:?}");
        assert!(worst.as_millis() <= 300, "{worst:?}");
        assert!(reopened.as_millis() <= 300, "{reopened:?}");
        std::fs::remove_dir_all(root).ok();
    }
}
