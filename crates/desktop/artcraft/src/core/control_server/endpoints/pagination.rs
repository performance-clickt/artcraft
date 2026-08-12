use std::collections::HashMap;
use url::form_urlencoded;

/// Shared `limit` / `cursor` handling for the control server's list endpoints.
///
/// The control protocol exposes an opaque string cursor. Behind it is a zero-based page index,
/// because both backing sources page that way: the tasks SQLite listing is read whole and sliced
/// in memory, and the library listing is paged by page index upstream. Keeping the cursor opaque
/// in the protocol means the encoding can change later without breaking clients.
pub const DEFAULT_PAGE_LIMIT: usize = 50;
pub const MAX_PAGE_LIMIT: usize = 200;

const INVALID_CURSOR_MESSAGE: &str = "The `cursor` parameter is not a cursor this server issued.";
const INVALID_LIMIT_MESSAGE: &str = "The `limit` parameter must be a non-negative whole number.";

/// Parses a raw query string into its key/value pairs.
///
/// NB: The list endpoints read the query this way rather than through axum's `Query` extractor on
/// purpose. An extractor rejection is emitted by axum itself as bare text, which would be the one
/// response on this server that is not a control envelope; parsing here keeps every malformed
/// parameter inside `BAD_REQUEST`. Last value wins for a repeated key.
pub fn parse_raw_query(maybe_raw_query: Option<&str>) -> HashMap<String, String> {
  let Some(raw_query) = maybe_raw_query else {
    return HashMap::new();
  };

  form_urlencoded::parse(raw_query.as_bytes())
      .map(|(key, value)| (key.into_owned(), value.into_owned()))
      .collect()
}

/// A blank `limit=` is treated as "unset" rather than an error, since query builders commonly
/// emit empty values for absent options.
pub fn parse_page_limit(maybe_limit: Option<&str>) -> Result<usize, &'static str> {
  let Some(limit) = maybe_limit.map(str::trim).filter(|limit| !limit.is_empty()) else {
    return Ok(DEFAULT_PAGE_LIMIT);
  };

  let limit = limit.parse::<usize>()
      .map_err(|_| INVALID_LIMIT_MESSAGE)?;

  Ok(resolve_page_limit(Some(limit)))
}

/// Clamps a caller-supplied `limit` into the supported range rather than erroring: an agent
/// asking for 10_000 rows wants "as many as you can give me", not a 400.
pub fn resolve_page_limit(maybe_limit: Option<usize>) -> usize {
  match maybe_limit {
    None => DEFAULT_PAGE_LIMIT,
    Some(0) => 1,
    Some(limit) => limit.min(MAX_PAGE_LIMIT),
  }
}

/// A missing cursor is the first page. A malformed cursor is an error rather than a silent
/// reset to page zero, which would make a client's paging loop repeat forever.
pub fn parse_page_cursor(maybe_cursor: Option<&str>) -> Result<usize, &'static str> {
  let Some(cursor) = maybe_cursor else {
    return Ok(0);
  };

  let cursor = cursor.trim();

  if cursor.is_empty() {
    return Ok(0);
  }

  cursor.parse::<usize>()
      .map_err(|_| INVALID_CURSOR_MESSAGE)
}

pub fn encode_page_cursor(page_index: usize) -> String {
  page_index.to_string()
}

/// Slices an already-ordered, fully-materialized list into one page.
/// Returns the page plus the cursor for the following page, if any rows remain.
pub fn take_page<T>(items: Vec<T>, page_index: usize, limit: usize) -> (Vec<T>, Option<String>) {
  let offset = page_index.saturating_mul(limit);

  if offset >= items.len() {
    return (Vec::new(), None);
  }

  let has_next_page = items.len() > offset.saturating_add(limit);

  let page = items.into_iter()
      .skip(offset)
      .take(limit)
      .collect();

  let maybe_next_cursor = has_next_page
      .then(|| encode_page_cursor(page_index + 1));

  (page, maybe_next_cursor)
}

#[cfg(test)]
mod tests {
  use super::*;

  mod limit_tests {
    use super::*;

    #[test]
    fn test_missing_limit_uses_the_default() {
      assert_eq!(resolve_page_limit(None), DEFAULT_PAGE_LIMIT);
      assert_eq!(parse_page_limit(None), Ok(DEFAULT_PAGE_LIMIT));
      assert_eq!(parse_page_limit(Some("")), Ok(DEFAULT_PAGE_LIMIT));
    }

    #[test]
    fn test_limit_is_clamped_into_range() {
      assert_eq!(resolve_page_limit(Some(0)), 1);
      assert_eq!(resolve_page_limit(Some(7)), 7);
      assert_eq!(resolve_page_limit(Some(MAX_PAGE_LIMIT + 1)), MAX_PAGE_LIMIT);
      assert_eq!(parse_page_limit(Some("100000")), Ok(MAX_PAGE_LIMIT));
    }

    #[test]
    fn test_malformed_limit_is_rejected() {
      assert!(parse_page_limit(Some("lots")).is_err());
      assert!(parse_page_limit(Some("-5")).is_err());
    }
  }

  mod raw_query_tests {
    use super::*;

    #[test]
    fn test_missing_query_is_empty() {
      assert!(parse_raw_query(None).is_empty());
    }

    #[test]
    fn test_values_are_percent_decoded() {
      let params = parse_raw_query(Some("search=red+car&limit=3"));

      assert_eq!(params.get("search").map(String::as_str), Some("red car"));
      assert_eq!(params.get("limit").map(String::as_str), Some("3"));
    }
  }

  mod cursor_tests {
    use super::*;

    #[test]
    fn test_missing_and_blank_cursors_are_the_first_page() {
      assert_eq!(parse_page_cursor(None), Ok(0));
      assert_eq!(parse_page_cursor(Some("")), Ok(0));
      assert_eq!(parse_page_cursor(Some("  ")), Ok(0));
    }

    #[test]
    fn test_round_trip() {
      let cursor = encode_page_cursor(4);
      assert_eq!(parse_page_cursor(Some(&cursor)), Ok(4));
    }

    #[test]
    fn test_malformed_cursor_is_rejected() {
      assert!(parse_page_cursor(Some("banana")).is_err());
      assert!(parse_page_cursor(Some("-1")).is_err());
    }
  }

  mod take_page_tests {
    use super::*;

    #[test]
    fn test_first_page_reports_a_next_cursor() {
      let (page, maybe_next) = take_page(vec![1, 2, 3, 4, 5], 0, 2);

      assert_eq!(page, vec![1, 2]);
      assert_eq!(maybe_next.as_deref(), Some("1"));
    }

    #[test]
    fn test_final_partial_page_has_no_next_cursor() {
      let (page, maybe_next) = take_page(vec![1, 2, 3, 4, 5], 2, 2);

      assert_eq!(page, vec![5]);
      assert_eq!(maybe_next, None);
    }

    #[test]
    fn test_exactly_full_final_page_has_no_next_cursor() {
      let (page, maybe_next) = take_page(vec![1, 2, 3, 4], 1, 2);

      assert_eq!(page, vec![3, 4]);
      assert_eq!(maybe_next, None);
    }

    #[test]
    fn test_page_past_the_end_is_empty() {
      let (page, maybe_next) = take_page(vec![1, 2, 3], 9, 2);

      assert!(page.is_empty());
      assert_eq!(maybe_next, None);
    }
  }
}
