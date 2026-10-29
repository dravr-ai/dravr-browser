# Changelog

### Breaking

- `CookieData` has a new public field, `expires: Option<f64>`: the cookie's
  expiry in seconds since the Unix epoch as CDP reports it, `None` for a
  session cookie. A struct literal naming every field must add it
  (`expires: None` keeps the old meaning). Serialized sessions are unaffected
  both ways: a stored cookie without the field reads back as `None`, and a
  session cookie writes no `expires` key.

### Added

- `capture_browser_session(&Browser)`: captures the browser context's whole
  cookie jar through CDP `Storage.getCookies`, every host included.
  `capture_session(&Page)` stays page-scoped (`Network.getCookies` for the
  page's URL and its frames) and leaves out cookies set on another host of the
  same login, such as an SSO domain.
- `inject_cookies` sets each cookie's expiry when it has one, so a persistent
  cookie stays persistent in the browser it is replayed into.
