//! Bounded FTS5 pages on the serialized background worker; no documents are loaded.
use super::*;
use notes_core::library::{Page, Query, Scope, Sort, Summary, prefix_query};
use rusqlite::types::Value;
use std::time::Instant;

const PROGRESS_OPS: i32 = 1_000;
const MAX_PROGRESS_CHECKS: usize = 2_000;
const MAX_QUERY_TIME: Duration = Duration::from_millis(150);

fn sort_sql(sort: Sort) -> &'static str {
    match sort {
        Sort::Edited => "n.pinned DESC,n.updated_at DESC,n.id",
        Sort::Created => "n.pinned DESC,n.created_at DESC,n.id",
        Sort::Title => "n.pinned DESC,n.title='',n.title COLLATE NOCASE,n.updated_at DESC,n.id",
    }
}
fn bound_snippet(text: String) -> String {
    if text.encode_utf16().count() <= 160 {
        return text;
    }
    let mut units = 0;
    let end = text.char_indices().find_map(|(index, ch)| {
        units += ch.len_utf16();
        (units > 159).then_some(index)
    });
    match end {
        Some(end) => format!("{}…", &text[..end]),
        None => text,
    }
}
impl Store {
    pub fn query(&self, query: &Query) -> Result<Page> {
        self.query_with_budget(query, MAX_QUERY_TIME)
    }
    fn query_with_budget(&self, query: &Query, time: Duration) -> Result<Page> {
        query.validate()?;
        let started = Instant::now();
        let mut checks = 0;
        self.db.progress_handler(
            PROGRESS_OPS,
            Some(move || {
                checks += 1;
                checks >= MAX_PROGRESS_CHECKS || started.elapsed() >= time
            }),
        );
        let result = self.query_inner(query);
        self.db.progress_handler(0, None::<fn() -> bool>);
        result.context("Could not finish this bounded query. Narrow the search or scope and retry.")
    }
    fn query_inner(&self, query: &Query) -> Result<Page> {
        let terms = prefix_query(&query.text)?;
        let searching = !query.text.trim().is_empty();
        if searching && terms.is_empty() {
            return Ok(Page::default());
        }
        let snippet = if searching {
            "substr(snippet(notes_fts,-1,'','','…',12),1,512)"
        } else {
            "substr(n.snippet,1,512)"
        };
        let mut sql = format!(
            "SELECT n.id,n.folder_id,substr(n.title,1,512),{snippet},n.created_at,n.updated_at,n.pinned,n.revision,n.deleted_at FROM "
        );
        sql.push_str(if searching {
            "notes_fts JOIN notes n ON n.rowid=notes_fts.rowid WHERE notes_fts MATCH ? AND "
        } else {
            "notes n WHERE "
        });
        let mut values = vec![];
        if searching {
            values.push(Value::Text(terms));
        }
        sql.push_str(if query.scope == Scope::Trash {
            "n.deleted_at IS NOT NULL"
        } else {
            "n.deleted_at IS NULL"
        });
        match &query.scope {
            Scope::Favorites => sql.push_str(" AND n.pinned!=0"),
            Scope::Folder(folder) => {
                sql.push_str(" AND n.folder_id IS ?");
                values.push(
                    folder
                        .as_ref()
                        .map_or(Value::Null, |id| Value::Text(id.clone())),
                );
            }
            _ => {}
        }
        sql.push_str(" ORDER BY ");
        if searching {
            sql.push_str("bm25(notes_fts,4.0,1.0),");
        }
        sql.push_str(if query.scope == Scope::Trash {
            "n.deleted_at DESC,n.id"
        } else {
            sort_sql(query.sort)
        });
        sql.push_str(" LIMIT ? OFFSET ?");
        values.push(Value::Integer(i64::from(query.limit) + 1));
        values.push(Value::Integer(i64::from(query.offset)));
        let mut statement = self.db.prepare(&sql)?;
        let mut rows = statement
            .query_map(rusqlite::params_from_iter(values), |r| {
                Ok(Summary {
                    id: r.get(0)?,
                    folder: r.get(1)?,
                    title: r.get(2)?,
                    snippet: bound_snippet(r.get(3)?),
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                    pinned: r.get(6)?,
                    revision: r.get(7)?,
                    deleted_at: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let has_more = rows.len() > usize::from(query.limit);
        rows.truncate(usize::from(query.limit));
        Ok(Page { rows, has_more })
    }
}

#[cfg(test)]
#[path = "search/tests.rs"]
mod tests;
