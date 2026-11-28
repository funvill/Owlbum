//! Messages for the query area: running a query and reading its rows.

/// `bad_query`: the query sent by the page could not be read at all.
pub const UNREADABLE: &str = "Owlbum couldn't understand that search. Try clearing the filter.";

/// `bad_query`: the query asks for something this version cannot do yet.
/// `what` names it ("the timeline view", "sorting by score").
pub fn not_yet(what: impl std::fmt::Display) -> String {
    format!("Owlbum can't show {what} yet.")
}

/// `query_expired`: the token was dropped; the page runs the query again.
pub const EXPIRED: &str = "The list of photos changed. Showing it again.";

/// What was being done, for an `internal` error while reading rows.
pub const READING_ROWS: &str = "reading the photos to show";

/// `bad_query`: a filter value that is not allowed (a date that is not a
/// real day, a blank tag or person).
pub fn bad_value(value: &str) -> String {
    format!("Owlbum couldn't use \"{value}\" as a filter. Try clearing the filter.")
}

/// The label of the open scope's own folder, in the folder list and its
/// chip (docs/05-organising.md, "The list of filters").
pub const TOP_FOLDER: &str = "Top folder";

/// Chip texts (docs/05-organising.md, "The list of filters").
pub mod chip {
    /// `star: yes`.
    pub const STARRED: &str = "Starred";
    /// `star: no`.
    pub const NOT_STARRED: &str = "Not starred";
    /// `shape: portrait`.
    pub const PORTRAIT: &str = "Portrait";
    /// `shape: landscape`.
    pub const LANDSCAPE: &str = "Landscape";
    /// `hidden: show`.
    pub const HIDDEN_SHOWN: &str = "Hidden shown";
    /// `hidden: only`.
    pub const ONLY_HIDDEN: &str = "Only hidden";
    /// `has_location`.
    pub const HAS_LOCATION: &str = "Has location";
    /// `has_people`.
    pub const HAS_PEOPLE: &str = "Has people";
    /// `low_quality`.
    pub const LOW_QUALITY: &str = "Low quality";
    /// `near_duplicates`.
    pub const NEAR_DUPLICATES: &str = "Near-duplicates";

    /// A kind of photo in the media type chip, as a plural.
    fn kind_plural(kind: crate::query::KindFilter) -> &'static str {
        use crate::query::KindFilter as K;
        match kind {
            K::Still => "Stills",
            K::Animated => "Animated",
            K::Raw => "RAW",
            K::Video => "Videos",
        }
    }

    /// The media type chip (owner's feedback, #165): "No videos" when one
    /// kind is left out, else the kinds shown, "Stills, RAW".
    pub fn kinds(kinds: &[crate::query::KindFilter]) -> String {
        use crate::query::KindFilter as K;
        let left_out: Vec<K> = K::ALL.into_iter().filter(|k| !kinds.contains(k)).collect();
        if let [one] = left_out.as_slice() {
            let what = kind_plural(*one);
            return match one {
                K::Raw => format!("No {what}"),
                _ => format!("No {}", what.to_lowercase()),
            };
        }
        kinds
            .iter()
            .map(|k| kind_plural(*k))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The search text's chip (owner's feedback, issue 196).
    pub fn search(text: &str) -> String {
        format!("Photos with '{text}'")
    }

    /// The one chip of `from` and `to`: "{from} → {to}", "From {from}" or
    /// "Until {to}".
    pub fn dates(from: Option<&str>, to: Option<&str>) -> String {
        match (from, to) {
            (Some(from), Some(to)) => format!("{from} \u{2192} {to}"),
            (Some(from), None) => format!("From {from}"),
            (None, Some(to)) => format!("Until {to}"),
            (None, None) => String::new(),
        }
    }
}

/// The search box's query words (docs/05-organising.md, "Query words in the
/// search box").
pub mod words {
    /// The hint line's message for a word with a known key and a bad value.
    pub fn error(word: &str, hint: &str) -> String {
        format!("\"{word}\" isn't a filter: {hint}")
    }

    /// `star:`.
    pub const STAR: &str = "try star:yes or star:no";
    /// `shape:`.
    pub const SHAPE: &str = "try shape:portrait or shape:landscape";
    /// `hidden:`.
    pub const HIDDEN: &str = "try hidden:show or hidden:only";
    /// `from:`.
    pub const FROM: &str = "try from:YYYY-MM-DD";
    /// `to:`.
    pub const TO: &str = "try to:YYYY-MM-DD";
    /// `from:` after `to:`, or `to:` before `from:`.
    pub const FROM_AFTER_TO: &str = "from must not be after to";
    /// `has:`.
    pub const HAS: &str = "try has:location or has:people";
    /// `is:`.
    pub const IS: &str = "try is:lowquality or is:duplicate";
    /// `tag:`.
    pub const TAG: &str = "try tag:<word>";
    /// `object:`.
    pub const OBJECT: &str = "try object:<word>";
    /// `kind:` and `no:`.
    pub const KIND: &str = "try kind:still, animated, raw or video";

    /// `format:` naming no format Owlbum reads.
    pub fn no_format(value: &str) -> String {
        format!("no format named {value}")
    }

    /// `folder:` naming no folder.
    pub fn no_folder(value: &str) -> String {
        format!("no folder named {value}")
    }

    /// `person:` naming nobody in the album.
    pub fn no_person(value: &str) -> String {
        format!("no person named {value}")
    }
}
