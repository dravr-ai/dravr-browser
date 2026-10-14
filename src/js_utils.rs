// ABOUTME: JavaScript string escaping utilities for safe embedding in CDP evaluate calls
// ABOUTME: Used to build JS snippets that interpolate selectors and user-provided values
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

/// Escape a string for safe embedding in JS double-quoted strings.
#[must_use]
pub fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Escape a CSS selector for embedding in JS double-quoted strings.
#[must_use]
pub fn escape_js_selector(s: &str) -> String {
    escape_js_string(s)
}

/// The one way every element helper in [`crate::input`] reads a selector
/// list, as JavaScript function declarations.
///
/// Public so a consumer that reads the page itself — a script reporting which
/// field a form filled — resolves a selector list exactly as the helpers that
/// filled it did, rather than with a reading of its own that can disagree.
/// Embed it in the script's own scope, ahead of the code that calls it:
///
/// - `selectorParts(list)` splits the list at its top-level commas only. A
///   comma inside parentheses (`:is()`, `:not()`), inside brackets or a quoted
///   attribute value, or escaped with a backslash, stays in its part. A list
///   whose parentheses, brackets or quotes never close is split at every
///   comma, so its unclosed part cannot swallow the parts after it. Parts are
///   trimmed, and an empty part is dropped.
/// - `selectorMatches(list)` is every element each part matches, part by part
///   in the order given and in document order within a part. A part the
///   browser cannot parse matches nothing, so it never fails the read of the
///   others. A `text:` part matches the buttons, links and `[role=button]`
///   elements whose text contains what follows the prefix.
/// - `isRendered(el)` is whether an element is on the page for a user to act
///   on: a non-empty box, and `checkVisibility` with `visibility` checked — or,
///   in a browser without `checkVisibility`, the element's computed `display`
///   and `visibility`. Opacity is no bar: a transparent input laid over the
///   boxes a code step draws (`COROS`) is what takes the typing.
///   `offsetParent` is not used: it is null for a `position: fixed` element,
///   such as a toast, that is plainly on screen.
/// - `isPainted(el)` is whether a user can read an element: rendered, and not
///   transparent (`checkVisibility` with opacity checked, or the computed
///   `opacity`). The page's `<html>` and `<body>` are never painted elements:
///   their text is the whole page.
/// - `locateElement(list)` is the element a helper acts on: the first rendered
///   match, or the first match when none is rendered, or `null`.
pub const SELECTOR_LIST_JS: &str = r#"
function selectorParts(list) {
    var parts = [];
    var depth = 0;
    var quote = '';
    var start = 0;
    for (var i = 0; i < list.length; i++) {
        var c = list.charAt(i);
        if (c === '\\') { i++; continue; }
        if (quote) { if (c === quote) { quote = ''; } continue; }
        if (depth > 0 && (c === '"' || c === "'")) { quote = c; continue; }
        if (c === '(' || c === '[') { depth++; }
        else if ((c === ')' || c === ']') && depth > 0) { depth--; }
        else if (c === ',' && depth === 0) { parts.push(list.slice(start, i)); start = i + 1; }
    }
    parts.push(list.slice(start));
    if (depth > 0 || quote) { parts = list.split(','); }
    return parts.map(function (p) { return p.trim(); }).filter(function (p) { return p.length > 0; });
}
function selectorMatches(list) {
    var found = [];
    var parts = selectorParts(list);
    for (var i = 0; i < parts.length; i++) {
        var part = parts[i];
        var matches;
        if (part.indexOf('text:') === 0) {
            var text = part.substring(5);
            matches = Array.prototype.filter.call(
                document.querySelectorAll('button, a, [role=button]'),
                function (el) { return el.textContent.trim().indexOf(text) !== -1; }
            );
        } else {
            try { matches = document.querySelectorAll(part); } catch (e) { continue; }
        }
        for (var j = 0; j < matches.length; j++) { found.push(matches[j]); }
    }
    return found;
}
function isRendered(el) {
    var box = el.getBoundingClientRect();
    if (box.width === 0 || box.height === 0) { return false; }
    if (el.checkVisibility) { return el.checkVisibility({ checkVisibilityCSS: true }); }
    var style = getComputedStyle(el);
    return style.display !== 'none' && style.visibility !== 'hidden';
}
function isPainted(el) {
    if (el === document.documentElement || el === document.body) { return false; }
    if (!isRendered(el)) { return false; }
    if (el.checkVisibility) { return el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }); }
    return getComputedStyle(el).opacity !== '0';
}
function locateElement(list) {
    var matches = selectorMatches(list);
    for (var i = 0; i < matches.length; i++) {
        if (isRendered(matches[i])) { return matches[i]; }
    }
    return matches.length > 0 ? matches[0] : null;
}
"#;

/// A script that runs `body` — statements ending in a `return` — inside an
/// immediately invoked function with [`SELECTOR_LIST_JS`] in scope, and
/// evaluates to what `body` returns.
pub(crate) fn with_selector_list(body: &str) -> String {
    format!("(function() {{{SELECTOR_LIST_JS}{body}\n}})()")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_plain_string() {
        assert_eq!(escape_js_string("hello"), "hello");
    }

    #[test]
    fn escape_quotes_and_backslashes() {
        assert_eq!(escape_js_string(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn escape_newlines() {
        assert_eq!(escape_js_string("line1\nline2\r"), "line1\\nline2\\r");
    }

    #[test]
    fn escape_selector_with_brackets() {
        assert_eq!(
            escape_js_selector(r#"input[name="email"]"#),
            r#"input[name=\"email\"]"#
        );
    }
}
