//! Cursor pagination shared by every list use-case.
//!
//! Keyset, not offset. `OFFSET` re-scans the rows it skips and, worse,
//! silently skips or repeats rows whenever the underlying data shifts
//! between two page requests -- which for a live inventory is most of the
//! time. A cursor names the last row a client actually saw, so the next
//! page continues from there regardless of what changed.

use std::{fmt, str::FromStr};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use db::{
    PublicId,
    chrono::{DateTime, TimeZone, Utc},
};
use thiserror::Error;
use uuid::Uuid;

/// Page size when the client does not ask for one.
pub const DEFAULT_PAGE_SIZE: u16 = 50;
/// Hard ceiling on a page, whatever the client asks for.
///
/// A limit is an instruction to the database about how much work to do,
/// so it is exactly the kind of client input that must be bounded before
/// it reaches one. Out-of-range values are clamped rather than rejected:
/// there is no sensible way for a caller to misuse `limit=1000000` and
/// nothing useful to tell them beyond "you got 200".
pub const MAX_PAGE_SIZE: u16 = 200;

/// A page of results plus the cursor that continues it.
///
/// `next_cursor` is `None` only when the backing query returned fewer
/// rows than were asked for, which is the one reliable signal that the
/// end was reached. A full page always yields a cursor, even when it
/// happens to be the last one -- the alternative is an extra count query
/// on every request to find out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Cursor>,
}

impl<T> Page<T> {
    /// Builds a page from rows already limited to `requested`, deriving
    /// the cursor from the last row via `key`.
    pub fn new(items: Vec<T>, requested: u16, key: impl Fn(&T) -> Cursor) -> Self {
        let next_cursor = (items.len() == usize::from(requested))
            .then(|| items.last().map(&key))
            .flatten();
        Self { items, next_cursor }
    }

    pub fn map<U>(self, transform: impl Fn(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(transform).collect(),
            next_cursor: self.next_cursor,
        }
    }
}

/// An opaque continuation token.
///
/// Opaque on purpose. A client that parses a cursor is a client that
/// depends on the sort key, which then cannot change without breaking
/// it. The encoding here is not a security measure -- it holds only data
/// the client was already shown -- but it does keep the contract honest:
/// the only valid thing to do with a cursor is hand it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor(String);

impl Cursor {
    /// A cursor over a public UUID, for catalog listings ordered by it.
    pub fn from_public_id(public_id: PublicId) -> Self {
        Self(URL_SAFE_NO_PAD.encode(public_id.get().as_bytes()))
    }

    /// A cursor over `(created_at, public_id)`, for listings ordered
    /// newest-first where the timestamp alone is not unique.
    pub fn from_timestamped(created_at: DateTime<Utc>, public_id: PublicId) -> Self {
        let mut bytes = Vec::with_capacity(24);
        bytes.extend_from_slice(&created_at.timestamp_micros().to_be_bytes());
        bytes.extend_from_slice(public_id.get().as_bytes());
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn decode_public_id(&self) -> Result<PublicId, CursorError> {
        let bytes = self.decode()?;
        let bytes: [u8; 16] = bytes.try_into().map_err(|_| CursorError)?;
        Ok(PublicId::new(Uuid::from_bytes(bytes)))
    }

    pub fn decode_timestamped(&self) -> Result<(DateTime<Utc>, PublicId), CursorError> {
        let bytes = self.decode()?;
        let bytes: [u8; 24] = bytes.try_into().map_err(|_| CursorError)?;
        let (timestamp, uuid) = bytes.split_at(8);
        let micros = i64::from_be_bytes(timestamp.try_into().map_err(|_| CursorError)?);
        let created_at = Utc.timestamp_micros(micros).single().ok_or(CursorError)?;
        let uuid: [u8; 16] = uuid.try_into().map_err(|_| CursorError)?;
        Ok((created_at, PublicId::new(Uuid::from_bytes(uuid))))
    }

    fn decode(&self) -> Result<Vec<u8>, CursorError> {
        URL_SAFE_NO_PAD.decode(&self.0).map_err(|_| CursorError)
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for Cursor {
    type Err = CursorError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(CursorError);
        }
        Ok(Self(value.to_owned()))
    }
}

/// A cursor that does not decode.
///
/// Reported rather than ignored. Silently treating a corrupt cursor as
/// "start from the beginning" would hand the caller page one while they
/// believe they are reading page nine, and they would never find out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("the cursor is not valid")]
pub struct CursorError;

/// Clamps a client-supplied page size into the accepted range.
pub fn page_size(requested: Option<u16>) -> u16 {
    requested
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_size_is_always_within_bounds() {
        assert_eq!(page_size(None), DEFAULT_PAGE_SIZE);
        assert_eq!(page_size(Some(10)), 10);
        assert_eq!(page_size(Some(MAX_PAGE_SIZE)), MAX_PAGE_SIZE);
        // Clamped, not honoured: a limit is an instruction to the
        // database about how much work to do.
        assert_eq!(page_size(Some(u16::MAX)), MAX_PAGE_SIZE);
        // Zero would page forever without ever advancing.
        assert_eq!(page_size(Some(0)), 1);
    }

    #[test]
    fn a_public_id_cursor_round_trips() {
        let id = PublicId::new(Uuid::new_v4());
        let cursor = Cursor::from_public_id(id);
        assert_eq!(cursor.decode_public_id().expect("decodes"), id);
        // URL-safe and unpadded, so it survives a query string untouched.
        let rendered = cursor.to_string();
        assert!(
            !rendered.contains(['+', '/', '=']),
            "a cursor must not need escaping: {rendered}"
        );
    }

    #[test]
    fn a_timestamped_cursor_round_trips_with_microsecond_fidelity() {
        // PostgreSQL stores timestamptz to the microsecond, so a cursor
        // that rounds to the second would land on the wrong side of a
        // page boundary and skip or repeat rows.
        let created_at = Utc
            .timestamp_micros(1_726_000_000_123_456)
            .single()
            .unwrap();
        let id = PublicId::new(Uuid::new_v4());
        let cursor = Cursor::from_timestamped(created_at, id);

        let (decoded_at, decoded_id) = cursor.decode_timestamped().expect("decodes");
        assert_eq!(decoded_at, created_at);
        assert_eq!(decoded_at.timestamp_micros(), created_at.timestamp_micros());
        assert_eq!(decoded_id, id);
    }

    #[test]
    fn a_corrupt_cursor_is_an_error_rather_than_a_silent_reset() {
        // Every one of these would otherwise be answered with page one
        // while the caller believes they are reading page nine.
        for bad in ["!!!!", "AAAA", "not-base64-@@", "short"] {
            let cursor = Cursor::from_str(bad).expect("non-empty parses as a candidate");
            assert!(
                cursor.decode_public_id().is_err(),
                "{bad:?} must not decode as a public id"
            );
            assert!(
                cursor.decode_timestamped().is_err(),
                "{bad:?} must not decode as a timestamped key"
            );
        }
        assert!(Cursor::from_str("").is_err(), "an empty cursor is not one");
    }

    #[test]
    fn a_cursor_of_the_wrong_shape_does_not_decode_as_the_other_shape() {
        // The two encodings differ only in length; without the length
        // check a catalog cursor would decode as a garbage timestamp.
        let id = PublicId::new(Uuid::new_v4());
        assert!(
            Cursor::from_public_id(id).decode_timestamped().is_err(),
            "a 16-byte cursor is not a timestamped key"
        );
        assert!(
            Cursor::from_timestamped(Utc::now(), id)
                .decode_public_id()
                .is_err(),
            "a 24-byte cursor is not a bare public id"
        );
    }

    #[test]
    fn a_full_page_yields_a_cursor_and_a_short_one_ends_the_walk() {
        let ids: Vec<PublicId> = (0..3).map(|_| PublicId::new(Uuid::new_v4())).collect();

        let full = Page::new(ids.clone(), 3, |id| Cursor::from_public_id(*id));
        assert_eq!(
            full.next_cursor,
            Some(Cursor::from_public_id(ids[2])),
            "the cursor names the last row actually returned"
        );

        let short = Page::new(ids.clone(), 4, |id| Cursor::from_public_id(*id));
        assert_eq!(
            short.next_cursor, None,
            "fewer rows than asked for is the end"
        );

        let empty = Page::new(Vec::<PublicId>::new(), 4, |id| Cursor::from_public_id(*id));
        assert_eq!(empty.next_cursor, None);
    }
}
