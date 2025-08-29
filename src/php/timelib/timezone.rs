//! Time zone specifier.

use std::borrow::Cow;

/// A time zone specifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Timezone<'a> {
    /// A raw time zone offset in seconds. Positive values are to the east of
    /// the meridian.
    Offset(i32),
    /// A shorthand time zone alias, e.g. CEST, UTC, A.
    Alias(Cow<'a, str>),
    /// A full IANA time zone name.
    Named(Cow<'a, str>),
}

impl Timezone<'_> {
    /// Returns true if the time zone is in daylight saving time.
    pub(super) fn is_dst(&self) -> bool {
        match self {
            Timezone::Offset(_) => false,
            Timezone::Alias(name) => TzMapping::find(name).is_some_and(|info| info.is_dst),
            Timezone::Named(name) => tzdb_data::find_tz(name.as_bytes()).is_some_and(|info| {
                info.local_time_types()
                    .iter()
                    .any(tz::LocalTimeType::is_dst)
            }),
        }
    }

    /// The offset of the time zone, in standard time seconds.
    pub(super) fn offset(&self) -> i32 {
        match self {
            Timezone::Offset(offset) => *offset,
            Timezone::Alias(name) => TzMapping::find(name)
                .map_or(0, |info| info.offset - if info.is_dst { 3600 } else { 0 }),
            Timezone::Named(name) => tzdb_data::find_tz(name.as_bytes())
                .and_then(|info| {
                    info.local_time_types()
                        .last()
                        .map(tz::LocalTimeType::ut_offset)
                })
                .unwrap_or(0),
        }
    }
}

/// A table entry for a local time zone.
#[derive(Debug)]
pub(super) struct TzMapping {
    /// The local time zone alias.
    abbr: &'static str,
    /// Whether the alias indicates daylight saving time.
    is_dst: bool,
    /// The offset of the time zone, in seconds.
    offset: i32,
}

impl TzMapping {
    /// Finds a time zone in the global time zone alias table.
    pub(super) fn find(uc_name: &str) -> Option<&'static TzMapping> {
        TZ_MAPPING
            .binary_search_by(|item| item.abbr.cmp(uc_name))
            .ok()
            .map(|index| &TZ_MAPPING[index])
    }
}

/// The global time zone alias table.
static TZ_MAPPING: &[TzMapping] = include!(concat!(env!("OUT_DIR"), "/timelib_timezonemap.rs"));
