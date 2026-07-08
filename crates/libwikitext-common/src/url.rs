//! A bare-bones URL type.

use std::borrow::Cow;

/// A URL parsing error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The string does not conform to RFC 3986. Or the parser is broken. But
    /// probably the first thing.
    #[error(transparent)]
    Parse(#[from] peg::error::ParseError<peg::str::LineCol>),
    /// The URI is just, unreasonably, way, far, extremely too long.
    #[error("uri too long; must be under 64k")]
    Size(#[from] core::num::TryFromIntError),
}

/// A bare-bones URL containing scheme, authority, and path.
#[derive(Clone)]
pub struct Url {
    /// The buffer for the URL string.
    data: Cow<'static, str>,
    /// The start position of the [`fn@Self::authority`].
    authority: u16,
    /// The start position of the [`fn@Self::path`].
    path: u16,
    /// The start position of the [`fn@Self::query`].
    query: u16,
    /// The start position of the [`fn@Self::fragment`].
    fragment: u16,
}

impl Url {
    /// Creates a URL from its component parts.
    fn from_parts(
        data: Cow<'static, str>,
        scheme: &str,
        authority: &str,
        path: &str,
        query: &str,
    ) -> Result<Self, Error> {
        let mut index = scheme.len();
        let authority = {
            let at = u16::try_from(index)?;
            index += authority.len();
            at
        };
        let path = {
            let at = u16::try_from(index)?;
            index += path.len();
            at
        };
        let query = {
            let at = u16::try_from(index)?;
            index += query.len();
            at
        };
        let fragment = u16::try_from(index)?;
        Ok(Self {
            data,
            authority,
            path,
            query,
            fragment,
        })
    }

    /// Parses a URL from a static string.
    ///
    /// # Errors
    ///
    /// * `url` is not a valid URL, or is too long
    pub fn from_static(url: &'static str) -> Result<Self, Error> {
        let [scheme, authority, path, query] = rfc_3986ish::uri(url)?;
        Self::from_parts(Cow::Borrowed(url), scheme, authority, path, query)
    }

    /// Parses a URL from a string using lax parsing that allows
    /// non-percent-encoded characters in most positions.
    ///
    /// # Errors
    ///
    /// * `url` is not a valid URL, or is too long
    pub fn lax(url: &str) -> Result<Self, Error> {
        let [scheme, authority, path, query] = rfc_3986ish::lax(url)?;
        Self::from_parts(url.to_owned().into(), scheme, authority, path, query)
    }

    /// Gets the entire URL as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.data
    }

    /// Gets the authority, if one exists.
    ///
    /// ```text
    /// scheme://user:pass@example.com:1234/path?query#fragment
    ///        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    #[must_use]
    pub fn authority(&self) -> Option<&str> {
        (self.path != self.authority)
            .then_some(&self.data[usize::from(self.authority)..usize::from(self.path)])
    }

    /// Extends the path.
    ///
    /// # Panics
    ///
    /// * `path` makes the URL >64k
    #[must_use]
    pub fn extend_path(mut self, path: &str) -> Self {
        let mut at = usize::from(self.query);
        let mut delta = u16::try_from(path.len()).unwrap();
        if !self.path().ends_with('/') {
            self.data.to_mut().insert(at, '/');
            at += 1;
            delta += 1;
        }
        self.data.to_mut().insert_str(at, path);
        self.query = self.query.strict_add(delta);
        self.fragment = self.fragment.strict_add(delta);
        self
    }

    /// Gets the URL fragment.
    ///
    /// ```text
    /// scheme://user:pass@example.com:1234/path?query#fragment
    ///                                               ^^^^^^^^^
    /// ```
    #[must_use]
    pub fn fragment(&self) -> &str {
        &self.data[usize::from(self.fragment)..]
    }

    /// Returns true if this URL looks like it points to another origin.
    #[must_use]
    pub fn is_absolute(&self) -> bool {
        self.scheme().is_some() || self.authority().is_some()
    }

    /// Gets the URL path.
    ///
    /// ```text
    /// scheme://user:pass@example.com:1234/path?query#fragment
    ///                                    ^^^^^
    /// ```
    #[must_use]
    pub fn path(&self) -> &str {
        &self.data[usize::from(self.path)..usize::from(self.query)]
    }

    /// Gets the URL query.
    ///
    /// ```text
    /// scheme://user:pass@example.com:1234/path?query#fragment
    ///                                         ^^^^^^
    /// ```
    #[must_use]
    pub fn query(&self) -> &str {
        &self.data[usize::from(self.query)..usize::from(self.fragment)]
    }

    /// Gets the URL scheme, if one exists.
    ///
    /// ```text
    /// scheme://user:pass@example.com:1234/path?query#fragment
    /// ^^^^^^^
    /// ```
    #[must_use]
    pub fn scheme(&self) -> Option<&str> {
        (self.authority != 0).then_some(&self.data[..usize::from(self.authority)])
    }

    /// Sets the authority.
    ///
    /// # Panics
    ///
    /// * `authority` makes the URL >64k
    pub fn set_authority(&mut self, authority: &str) {
        let old_len = self.path - self.authority;
        let delta = u16::try_from(authority.len())
            .unwrap()
            .checked_signed_diff(old_len)
            .unwrap();
        self.data.to_mut().replace_range(
            usize::from(self.authority)..usize::from(self.path),
            authority,
        );
        self.path = self.path.strict_add_signed(delta);
        self.query = self.query.strict_add_signed(delta);
        self.fragment = self.fragment.strict_add_signed(delta);
    }
}

impl core::fmt::Debug for Url {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Url")
            .field("scheme", &self.scheme())
            .field("authority", &self.authority())
            .field("path", &self.path())
            .field("query", &self.query())
            .field("fragment", &self.fragment())
            .finish()
    }
}

impl core::fmt::Display for Url {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.data.fmt(f)
    }
}

impl core::str::FromStr for Url {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let [scheme, authority, path, query] = rfc_3986ish::uri(s)?;
        Self::from_parts(s.to_owned().into(), scheme, authority, path, query)
    }
}

peg::parser! {grammar rfc_3986ish() for str {
    pub rule lax() -> [&'input str; 4]
    = s:$(scheme()?) ap:lax_hier_part() q:$(("?" [^'#']*)?) ("#" [_]*)?
    { [s, ap.0, ap.1, q] }

    rule lax_hier_part() -> (&'input str, &'input str)
    = a:$("//" [^'/'|'?'|'#']*) p:$("/"? [^'?'|'#']*)
    { (a, p) }
    / a:$() p:$([^'?'|'#']*)
    { (a, p) }

    pub rule uri() -> [&'input str; 4]
    = s:$(scheme()?) ap:hier_part() q:$(query()?) fragment()?
    { [s, ap.0, ap.1, q] }

    rule hier_part() -> (&'input str, &'input str)
    = a:$("//" authority()) p:$(path_abempty())
    { (a, p) }
    / a:$() p:$(path_absolute())
    { (a, p) }
    / a:$() p:$(path_rootless())
    { (a, p) }
    / a:$() p:$(path_empty())
    { (a, p) }

    rule scheme() = alpha() (alpha() / digit() / ['+'|'-'|'.'])* ":"
    rule authority() = user_info()? host() port()?
    rule user_info() = (unreserved() / pct_encoded() / sub_delims() / ":")* "@"
    rule host() = ip_literal() / ipv4_address() / reg_name()*
    rule port() = ":" digit()*

    rule ip_literal()
    = "[" (ipv6_address() / ipv_future()) "]"

    rule ipv6_address()
    =                           h16c()*<6> ls32()
    /               h16()? "::" h16c()*<5> ls32()
    / (h16c()*<,1> h16())? "::" h16c()*<4> ls32()
    / (h16c()*<,2> h16())? "::" h16c()*<3> ls32()
    / (h16c()*<,3> h16())? "::" h16c()*<2> ls32()
    / (h16c()*<,4> h16())? "::" h16c()     ls32()
    / (h16c()*<,5> h16())? "::" h16()
    / (h16c()*<,6> h16())? "::"

    rule ipv_future()
    = "v" hexdig()+ "." (unreserved() / sub_delims() / ":")+

    rule h16()
    = hexdig()*<1,4>

    rule h16c()
    = h16() ":"

    rule ls32()
    = h16() ":" h16()
    / ipv4_address()

    rule ipv4_address()
    = dec_octet() ("." dec_octet())*<3>

    rule dec_octet()
    = "25" ['0'..='5']
    / "2"  ['0'..='4'] digit()
    / "1"  digit()*<1,2>
    / digit()

    rule path_abempty() = ("/" pchar()*)*
    rule path_absolute() = "/" (pchar()+ path_abempty())?
    rule path_rootless() = pchar()+ path_abempty()
    rule path_empty() = !pchar()
    rule pchar() = reg_name() / [':'|'@']

    rule query() = "?" (pchar() / ['/'|'?'])*
    rule fragment() = "#" (pchar() / ['/'|'?'])*

    rule reg_name() = unreserved() / pct_encoded() / sub_delims()

    rule pct_encoded() = "%" hexdig()*<2>

    rule unreserved() = alpha() / digit() / ['-'|'.'|'_'|'~']
    rule reserved() = gen_delims() / sub_delims()

    rule gen_delims() = [':'|'/'|'?'|'#'|'['|']'|'@']
    rule sub_delims() = ['!'|'$'|'&'|'\''|'('|')'|'*'|'+'|','|';'|'=']

    rule alpha() = ['A'..='Z'|'a'..='z']
    rule digit() = ['0'..='9']
    rule hexdig() = digit() / ['A'..='F'|'a'..='f']
}}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url() {
        let mut url = Url::from_static("http://example.com/?foo#bar").unwrap();
        assert_eq!(url.scheme(), Some("http:"));
        assert_eq!(url.authority(), Some("//example.com"));
        assert_eq!(url.path(), "/");
        assert_eq!(url.query(), "?foo");
        assert_eq!(url.fragment(), "#bar");

        // Shrink
        url.set_authority("//zombo.com");
        assert_eq!(url.authority(), Some("//zombo.com"));
        assert_eq!(url.path(), "/");
        assert_eq!(url.query(), "?foo");
        assert_eq!(url.fragment(), "#bar");

        // Grow
        url.set_authority("//example.com");
        assert_eq!(url.authority(), Some("//example.com"));
        assert_eq!(url.path(), "/");
        assert_eq!(url.query(), "?foo");
        assert_eq!(url.fragment(), "#bar");

        let url = url.extend_path("lol");
        assert_eq!(url.path(), "/lol");
        assert_eq!(url.query(), "?foo");
        assert_eq!(url.fragment(), "#bar");
        let url = url.extend_path("kek");
        assert_eq!(url.path(), "/lol/kek");
        assert_eq!(url.query(), "?foo");
        assert_eq!(url.fragment(), "#bar");
    }
}
