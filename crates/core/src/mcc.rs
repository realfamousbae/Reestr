//! MCC, the merchant category code of a purchase (D20). A code is four digits and stays text:
//! leading zeros matter (0742).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Four ASCII digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Mcc([u8; 4]);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("an MCC is exactly 4 digits, got {0:?}")]
pub struct InvalidMcc(pub String);

impl Mcc {
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}

impl FromStr for Mcc {
    type Err = InvalidMcc;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <[u8; 4]>::try_from(s.as_bytes())
            .ok()
            .filter(|digits| digits.iter().all(u8::is_ascii_digit))
            .map(Self)
            .ok_or_else(|| InvalidMcc(s.to_owned()))
    }
}

impl TryFrom<String> for Mcc {
    type Error = InvalidMcc;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Mcc> for String {
    fn from(mcc: Mcc) -> Self {
        mcc.as_str().to_owned()
    }
}

impl fmt::Debug for Mcc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Mcc({self})")
    }
}

impl fmt::Display for Mcc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A typed code: spaces around it do not count, an empty one clears the field.
pub fn parse_mcc(text: Option<&str>) -> Result<Option<Mcc>, InvalidMcc> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::parse)
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_digits_with_leading_zeros() {
        let mcc: Mcc = "0742".parse().unwrap();
        assert_eq!(mcc.as_str(), "0742");
        assert_eq!(mcc.to_string(), "0742");
        assert_eq!(serde_json::to_string(&mcc).unwrap(), r#""0742""#);
        assert_eq!(serde_json::from_str::<Mcc>(r#""0742""#).unwrap(), mcc);
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in ["742", "07420", "54a1", "٠٧٤٢", "5 11", "-541", "5411.0"] {
            assert_eq!(bad.parse::<Mcc>(), Err(InvalidMcc(bad.to_owned())), "{bad}");
        }
        assert!(serde_json::from_str::<Mcc>(r#""742""#).is_err());
    }

    #[test]
    fn typed_text() {
        assert_eq!(parse_mcc(None), Ok(None));
        assert_eq!(parse_mcc(Some("   ")), Ok(None));
        assert_eq!(parse_mcc(Some(" 5411 ")), Ok(Some("5411".parse().unwrap())));
        assert_eq!(parse_mcc(Some("54")), Err(InvalidMcc("54".to_owned())));
    }
}
