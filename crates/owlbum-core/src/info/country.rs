//! Country outlines: which country a place is in, and the country search
//! (docs/04-browsing.md, "How typed fields save").

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One answer of `app_place_search`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CountryMatch {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

pub(crate) struct Country {
    pub name: String,
}

impl Country {
    pub(crate) fn centre(&self) -> (f64, f64) {
        unimplemented!()
    }
}

pub(crate) struct Outlines {
    pub countries: Vec<Country>,
}

pub(crate) fn outlines() -> &'static Outlines {
    unimplemented!()
}

/// The country that contains a place.
pub fn country_at(_lat: f64, _lon: f64) -> Option<&'static str> {
    unimplemented!()
}

/// Countries whose name contains the text.
pub fn place_search(_text: &str) -> Vec<CountryMatch> {
    unimplemented!()
}

#[cfg(test)]
mod tests;
