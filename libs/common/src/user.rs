use std::ops::Deref;

/// A user's login name (e.g. their GitHub `login`).
///
/// Stored as-is (no validation on read) so legacy rows can never fail
/// to decode; validation of new values happens at the edge.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Username(String);

impl Username {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for Username {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Username {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Username {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Username {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl sqlx::Type<sqlx::Postgres> for Username {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for Username {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        <String as sqlx::Encode<'q, sqlx::Postgres>>::encode_by_ref(&self.0, buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for Username {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        <String as sqlx::Decode<'r, sqlx::Postgres>>::decode(value).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deref_and_display_match_inner() {
        let u = Username::from("octocat");
        assert_eq!(&*u, "octocat");
        assert_eq!(u.as_str(), "octocat");
        assert_eq!(format!("{u}"), "octocat");
        assert_eq!(u.as_ref() as &str, "octocat");
    }

    #[test]
    fn serde_roundtrip() {
        let u = Username::from("octocat");
        let json = serde_json::to_string(&u).unwrap();
        assert_eq!(json, "\"octocat\"");
        let back: Username = serde_json::from_str(&json).unwrap();
        assert_eq!(back, u);
    }
}
