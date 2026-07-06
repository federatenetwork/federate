//! federate-search — indexing and querying public Federate pages.
//!
//! Policy (non-negotiable, applies to the official node and any third-party
//! search node claiming Federate compliance):
//! - NO ads
//! - NO tracking
//! - NO AI training on indexed content
//! - public pages only, with opt-out honored (`<meta name="federate"
//!   content="noindex">` or `<meta name="robots" content="noindex">`)

use federate_resolution::{Resolved, Resolver};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub const POLICY_NO_ADS: bool = true;
pub const POLICY_NO_TRACKING: bool = true;
pub const POLICY_NO_AI_TRAINING: bool = true;

// ---------------------------------------------------------------------------
// Opt-out + HTML extraction
// ---------------------------------------------------------------------------

/// Whether a page opted out of indexing.
pub fn opted_out(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    for tag in lower.split('<') {
        if !tag.starts_with("meta") {
            continue;
        }
        let is_robots = tag.contains(r#"name="robots""#) || tag.contains("name='robots'");
        let is_federate = tag.contains(r#"name="federate""#) || tag.contains("name='federate'");
        if (is_robots || is_federate) && tag.contains("noindex") {
            return true;
        }
    }
    false
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut skip_block = false;
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if !in_tag && bytes[i] == b'<' {
            in_tag = true;
            let rest = &lower[i..];
            if rest.starts_with("<script") || rest.starts_with("<style") {
                skip_block = true;
            } else if rest.starts_with("</script") || rest.starts_with("</style") {
                skip_block = false;
            }
        } else if in_tag && bytes[i] == b'>' {
            in_tag = false;
            out.push(' ');
        } else if !in_tag && !skip_block {
            out.push(html[i..].chars().next().unwrap());
            i += html[i..].chars().next().unwrap().len_utf8();
            continue;
        }
        i += 1;
    }
    out
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = html[start..].find('>')? + start + 1;
    let close = lower[open_end..].find("</title>")? + open_end;
    Some(html[open_end..close].trim().to_string())
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Index
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDoc {
    pub domain: String,
    pub path: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub domain: String,
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
}

#[derive(Default)]
pub struct SearchIndex {
    docs: Vec<PageDoc>,
    /// term -> doc index -> term frequency
    inverted: HashMap<String, HashMap<usize, u32>>,
}

impl SearchIndex {
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Index an HTML page. Returns false when the page opted out.
    pub fn index_html(&mut self, domain: &str, path: &str, html: &str) -> bool {
        if opted_out(html) {
            tracing::debug!("{domain}{path} opted out of indexing");
            return false;
        }
        let text = strip_tags(html);
        let title = extract_title(html).unwrap_or_else(|| domain.to_string());
        let doc_id = self.docs.len();
        for term in tokenize(&text).into_iter().chain(tokenize(&title)) {
            *self
                .inverted
                .entry(term)
                .or_default()
                .entry(doc_id)
                .or_insert(0) += 1;
        }
        self.docs.push(PageDoc {
            domain: domain.to_string(),
            path: path.to_string(),
            title,
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
        });
        true
    }

    /// Query: rank by summed term frequency, weighted by how many distinct
    /// query terms the doc matches.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let terms = tokenize(query);
        if terms.is_empty() {
            return vec![];
        }
        let mut scores: HashMap<usize, (f64, usize)> = HashMap::new();
        for term in &terms {
            if let Some(postings) = self.inverted.get(term) {
                for (&doc, &tf) in postings {
                    let entry = scores.entry(doc).or_insert((0.0, 0));
                    entry.0 += tf as f64;
                    entry.1 += 1;
                }
            }
        }
        let mut ranked: Vec<(usize, f64)> = scores
            .into_iter()
            .map(|(doc, (tf, matched))| (doc, tf * matched as f64))
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked
            .into_iter()
            .take(limit)
            .map(|(doc, score)| {
                let d = &self.docs[doc];
                let snippet: String = d.text.chars().take(180).collect();
                SearchResult {
                    domain: d.domain.clone(),
                    path: d.path.clone(),
                    title: d.title.clone(),
                    snippet,
                    score,
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Crawling via the shared resolution engine
// ---------------------------------------------------------------------------

/// Build an index of every HTML file of every active domain in the verified
/// root zone, through the same signature-verifying resolver everything else
/// uses. Unverifiable content is never indexed.
pub async fn index_from_resolver(resolver: &Resolver) -> federate_core::Result<SearchIndex> {
    let mut index = SearchIndex::default();
    let zone = resolver.root().await?;
    for (fqdn, rec) in &zone.domains {
        if !rec.status.is_resolvable() {
            continue;
        }
        // Walk every HTML file in the verified manifest, not just "/".
        let files = match resolver.site_files(fqdn).await {
            Ok(f) => f,
            Err(e) => {
                tracing::debug!("skipping {fqdn}: {e}");
                continue;
            }
        };
        let html_paths: Vec<String> = files
            .into_iter()
            .filter(|p| p.ends_with(".html") || p.ends_with(".htm"))
            .map(|p| format!("/{p}"))
            .collect();
        for path in html_paths {
            if let Ok(Resolved::Content { bytes, mime, .. }) = resolver.resolve(fqdn, &path).await {
                if mime.starts_with("text/html") {
                    if let Ok(html) = String::from_utf8(bytes) {
                        index.index_html(fqdn, &path, &html);
                    }
                }
            }
        }
    }
    tracing::info!("search index built: {} pages", index.len());
    Ok(index)
}

// ---------------------------------------------------------------------------
// HTTP API
// ---------------------------------------------------------------------------

pub fn router(index: Arc<RwLock<SearchIndex>>) -> axum::Router {
    use axum::extract::{Query, State};
    use axum::routing::get;
    use axum::Json;

    async fn search(
        State(index): State<Arc<RwLock<SearchIndex>>>,
        Query(q): Query<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        let query = q.get("q").cloned().unwrap_or_default();
        let results = index.read().await.search(&query, 20);
        Json(serde_json::json!({
            "query": query,
            "results": results,
            "policy": { "ads": false, "tracking": false, "ai_training": false },
        }))
    }

    async fn stats(State(index): State<Arc<RwLock<SearchIndex>>>) -> Json<serde_json::Value> {
        Json(serde_json::json!({ "pages": index.read().await.len() }))
    }

    axum::Router::new()
        .route("/v1/search", get(search))
        .route("/v1/search/stats", get(stats))
        .with_state(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_and_ranks_respecting_optout() {
        let mut idx = SearchIndex::default();
        assert!(idx.index_html(
            "home.fed",
            "/",
            "<html><title>Federate Home</title><body>welcome to the federate network</body></html>"
        ));
        assert!(!idx.index_html(
            "private.fed",
            "/",
            r#"<html><meta name="federate" content="noindex"><body>secret federate page</body></html>"#
        ));
        let results = idx.search("federate network", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].domain, "home.fed");
        assert_eq!(results[0].title, "Federate Home");
    }
}
