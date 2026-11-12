//! Bounds for one `s3_list_objects` call (IF-189).
//!
//! `max_keys` is only the size of a page. A listing is also bounded in
//! total, and an endpoint whose pages never end is abandoned: a truncated
//! page must carry a continuation token that has not been seen in this call,
//! and pages must keep producing entries.

use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::engine::types::Context;
use crate::util::limits::max_s3_list_objects;
use crate::util::node_config::{config_u64, config_usize_strict};

/// Keys S3 returns in one page when the caller sets no page size.
const DEFAULT_PAGE_KEYS: usize = 1_000;
/// Consecutive truncated pages without an entry that end the call. S3 may
/// return such a page; an endpoint that only returns them is not followed.
const MAX_EMPTY_PAGES: usize = 16;

/// What the caller asked for, checked against the process ceiling.
pub(super) struct ListBounds {
    page_keys: usize,
    max_entries: usize,
    /// `max_objects` was given: reaching it returns the token to continue
    /// from. Without it, a listing above the process ceiling fails rather
    /// than returning a part the flow never asked to page through.
    paging: bool,
}

impl ListBounds {
    pub(super) fn from_config(config: &serde_json::Value, ctx: &Context) -> Result<Self> {
        let ceiling = max_s3_list_objects();
        let max_objects = config_usize_strict(config, "max_objects", ctx)
            .map_err(|error| anyhow::anyhow!("s3_list_objects {error}"))?;
        if let Some(max_objects) = max_objects
            && !(1..=ceiling).contains(&max_objects)
        {
            bail!(
                "s3_list_objects 'max_objects' must be between 1 and {ceiling} (IRONFLOW_MAX_S3_LIST_OBJECTS)"
            );
        }
        let page_keys = config_u64(config, "max_keys", ctx)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_PAGE_KEYS);
        Ok(Self {
            page_keys,
            max_entries: max_objects.unwrap_or(ceiling),
            paging: max_objects.is_some(),
        })
    }
}

/// What to do after a page.
pub(super) enum Next {
    /// Request the page this token names.
    Page(String),
    /// The listing ended, or reached `max_objects` with `Some` token left.
    Done(Option<String>),
}

/// Progress of one listing across its pages.
pub(super) struct Pager {
    bounds: ListBounds,
    entries: usize,
    seen_tokens: HashSet<String>,
    empty_pages: usize,
}

impl Pager {
    /// `start_token` is the token the caller resumes from, if any.
    pub(super) fn new(bounds: ListBounds, start_token: Option<&str>) -> Self {
        Self {
            bounds,
            entries: 0,
            seen_tokens: start_token.map(str::to_string).into_iter().collect(),
            empty_pages: 0,
        }
    }

    /// Entries to ask for next: a page, cut to what the bound still admits,
    /// so the token returned with it continues exactly after the last entry.
    pub(super) fn page_keys(&self) -> i32 {
        let remaining = self.bounds.max_entries - self.entries;
        i32::try_from(self.bounds.page_keys.min(remaining)).unwrap_or(i32::MAX)
    }

    /// Account for a received page of `entries` objects and common prefixes
    /// before any of them is kept.
    pub(super) fn admit_page(&mut self, entries: usize) -> Result<()> {
        let requested = usize::try_from(self.page_keys()).unwrap_or(usize::MAX);
        if entries > requested {
            bail!(
                "s3_list_objects endpoint returned {entries} entries for a page of at most {requested}"
            );
        }
        self.entries += entries;
        self.empty_pages = if entries == 0 {
            self.empty_pages + 1
        } else {
            0
        };
        Ok(())
    }

    /// Decide what follows the page just admitted.
    pub(super) fn next(&mut self, truncated: bool, token: Option<&str>) -> Result<Next> {
        if !truncated {
            return Ok(Next::Done(None));
        }
        let Some(token) = token.filter(|token| !token.is_empty()) else {
            bail!(
                "s3_list_objects endpoint returned a truncated page without a continuation token"
            );
        };
        if !self.seen_tokens.insert(token.to_string()) {
            bail!(
                "s3_list_objects endpoint repeated a continuation token; the listing would not end"
            );
        }
        if self.entries >= self.bounds.max_entries {
            if self.bounds.paging {
                return Ok(Next::Done(Some(token.to_string())));
            }
            bail!(
                "s3_list_objects listing holds more than {} entries; page through it with 'max_objects' and 'continuation_token', or raise IRONFLOW_MAX_S3_LIST_OBJECTS",
                self.bounds.max_entries
            );
        }
        if self.empty_pages >= MAX_EMPTY_PAGES {
            bail!(
                "s3_list_objects endpoint returned {MAX_EMPTY_PAGES} consecutive truncated pages without an entry"
            );
        }
        Ok(Next::Page(token.to_string()))
    }
}
