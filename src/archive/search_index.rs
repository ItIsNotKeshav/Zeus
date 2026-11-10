//! A trigram index of the text a search matches, so finding a word reads
//! the index instead of every message's JSON.
//!
//! The index holds the same text as [`super::SEARCHED_TEXT`], one row per
//! message under the message's rowid. Triggers keep it in step with
//! `messages` from the moment it exists. The messages that were archived
//! before it are added a batch at a time on the worker's tick, so opening the
//! archive never waits for them; until the last batch the search scans as it
//! always did. A trigram finds any run of three or more characters, which is
//! what `LIKE '%…%'` found, so a shorter search keeps scanning too.

use super::{Archive, Result, SEARCHED_TEXT, params};

/// Created after `messages`. Each trigger reads the new row's JSON the way
/// the search does; JSON that does not parse is left out, as the scan skips it.
pub fn schema() -> String {
    let new_text = SEARCHED_TEXT.replace("json_extract(content,", "json_extract(new.content,");
    format!(
        "
CREATE VIRTUAL TABLE IF NOT EXISTS message_text USING fts5(body, tokenize = 'trigram');
CREATE TRIGGER IF NOT EXISTS message_text_insert AFTER INSERT ON messages BEGIN
    INSERT INTO message_text (rowid, body)
    SELECT new.rowid, {new_text} WHERE json_valid(new.content);
END;
CREATE TRIGGER IF NOT EXISTS message_text_update AFTER UPDATE OF content ON messages BEGIN
    DELETE FROM message_text WHERE rowid = old.rowid;
    INSERT INTO message_text (rowid, body)
    SELECT new.rowid, {new_text} WHERE json_valid(new.content);
END;
CREATE TRIGGER IF NOT EXISTS message_text_delete AFTER DELETE ON messages BEGIN
    DELETE FROM message_text WHERE rowid = old.rowid;
END;
"
    )
}

/// The highest rowid archived before the index, which the batches fill up to.
const CEILING: &str = "message_text_ceiling";
/// The rowid the batches have filled through.
const THROUGH: &str = "message_text_through";

/// The fewest characters a trigram can find.
pub const SHORTEST: usize = 3;

impl Archive {
    fn rowid_meta(&self, key: &str) -> Result<Option<i64>> {
        Ok(self.meta(key)?.and_then(|value| value.parse().ok()))
    }

    /// Adds up to `batch` messages archived before the index to it, and says
    /// whether the index now holds them all.
    pub fn fill_search_index(&self, batch: i64) -> Result<bool> {
        let ceiling = match self.rowid_meta(CEILING)? {
            Some(ceiling) => ceiling,
            None => {
                // Rows after this were indexed by the triggers. The first
                // fill after a clear starts over the same way.
                let ceiling: i64 = self.connection.query_row(
                    "SELECT COALESCE(MAX(rowid), 0) FROM messages",
                    [],
                    |row| row.get(0),
                )?;
                self.set_meta(CEILING, &ceiling.to_string())?;
                self.set_meta(THROUGH, "0")?;
                ceiling
            }
        };
        let through = self.rowid_meta(THROUGH)?.unwrap_or(0);
        if through >= ceiling {
            return Ok(true);
        }
        let until = ceiling.min(through.saturating_add(batch));
        // A row the triggers already indexed, one written again since, is
        // not indexed twice.
        self.connection.execute(
            &format!(
                "INSERT INTO message_text (rowid, body)
                 SELECT rowid, {SEARCHED_TEXT} FROM messages
                 WHERE rowid > ?1 AND rowid <= ?2 AND json_valid(content)
                 AND rowid NOT IN (SELECT rowid FROM message_text WHERE rowid > ?1 AND rowid <= ?2)"
            ),
            params![through, until],
        )?;
        self.set_meta(THROUGH, &until.to_string())?;
        Ok(until >= ceiling)
    }

    /// Whether every archived message is in the index.
    pub fn search_index_ready(&self) -> Result<bool> {
        Ok(
            match (self.rowid_meta(CEILING)?, self.rowid_meta(THROUGH)?) {
                (Some(ceiling), Some(through)) => through >= ceiling,
                _ => false,
            },
        )
    }
}

/// The FTS5 query that finds `needle` anywhere, as one quoted phrase, or
/// `None` when it is too short for a trigram.
pub fn phrase(needle: &str) -> Option<String> {
    let needle = needle.trim();
    (needle.chars().count() >= SHORTEST).then(|| format!("\"{}\"", needle.replace('"', "\"\"")))
}

#[cfg(test)]
mod tests {
    use super::super::tests::message;
    use super::*;
    use crate::model::Content;

    fn ids(archive: &Archive, needle: &str) -> Vec<String> {
        archive
            .search_messages(needle, 10)
            .expect("search")
            .into_iter()
            .map(|hit| hit.id)
            .collect()
    }

    fn text(archive: &Archive, id: &str, timestamp: i64, text: &str) {
        let mut row = message("chat", id, timestamp, false);
        row.content = Content::text(text);
        archive.insert_message(&row, None).expect("insert");
    }

    /// An archive from before the index scans until the batches have filled
    /// it, and finds the same messages either way.
    #[test]
    fn an_older_archive_is_indexed_in_batches_and_finds_the_same() {
        let archive = Archive::in_memory().expect("opens");
        archive.ensure_chat("chat", "Ada").expect("chat");
        text(&archive, "m1", 10, "The Difference Engine");
        text(&archive, "m2", 20, "an engine room");
        text(&archive, "m3", 30, "nothing of note");
        // As it was before this change: no rows in the index.
        archive
            .connection
            .execute("DELETE FROM message_text", [])
            .expect("empty the index");
        assert!(!archive.search_index_ready().unwrap());
        let scanned = ids(&archive, "ENGINE");
        assert_eq!(scanned, ["m2", "m1"]);
        let mut batches = 0;
        while !archive.fill_search_index(1).unwrap() {
            batches += 1;
            assert!(batches < 10, "the batches end");
        }
        assert!(archive.search_index_ready().unwrap());
        assert_eq!(ids(&archive, "ENGINE"), scanned, "the index finds the same");
        assert_eq!(ids(&archive, "ngin"), scanned, "inside a word too");
        let indexed: i64 = archive
            .connection
            .query_row("SELECT COUNT(*) FROM message_text", [], |row| row.get(0))
            .unwrap();
        assert_eq!(indexed, 3, "each message once");
    }

    /// Messages written, edited and deleted after the index keep it current.
    #[test]
    fn the_index_follows_new_edited_and_deleted_messages() {
        let archive = Archive::in_memory().expect("opens");
        archive.ensure_chat("chat", "Ada").expect("chat");
        while !archive.fill_search_index(100).unwrap() {}
        text(&archive, "m1", 10, "lunch at noon");
        assert_eq!(ids(&archive, "lunch"), ["m1"]);
        archive
            .connection
            .execute(
                "UPDATE messages SET content = ?1 WHERE id = 'm1'",
                params![serde_json::to_string(&Content::text("dinner at eight")).unwrap()],
            )
            .unwrap();
        assert!(ids(&archive, "lunch").is_empty(), "the old words are gone");
        assert_eq!(ids(&archive, "dinner"), ["m1"]);
        assert!(archive.delete_message("chat", "m1").unwrap());
        assert!(ids(&archive, "dinner").is_empty());
    }

    /// Two letters are too few for a trigram: the search scans, as before.
    #[test]
    fn a_short_search_still_finds_messages() {
        let archive = Archive::in_memory().expect("opens");
        archive.ensure_chat("chat", "Ada").expect("chat");
        text(&archive, "m1", 10, "ok then");
        while !archive.fill_search_index(100).unwrap() {}
        assert_eq!(phrase("ok"), None);
        assert_eq!(ids(&archive, "ok"), ["m1"]);
        assert_eq!(phrase(r#"say "hi""#).as_deref(), Some(r#""say ""hi""""#));
    }
}
