//! Interface languages. Strings in the code stay English and are the lookup keys; the craft-i18n
//! catalogs map them to display text when the UI is drawn. Command ids and labels, undo-history
//! labels, ribbon tab ids (`view.tab`, `ui.json`), Quick Access Toolbar labels, document text, file
//! names, the control channel, the CLI and MCP never see translated text, so scripts and agents
//! are unaffected. Only this crate knows about translation. A string without a translation shows in
//! English, so coverage grows one entry at a time, and a missing translation never fails a build, a
//! test or a release.
//!
//! The catalogs live in [storytold/craft-i18n](https://github.com/storytold/craft-i18n)
//! (`locales/<locale>/common.tsv` and `deckcraft.tsv`), not in this repository. `build.rs` embeds
//! them from `CRAFT_I18N_DIR` (CI and releases check out the SHA in `craft-i18n.rev`); without it,
//! or before craft-i18n has a `deckcraft.tsv`, the build is English only. Lookup, fallback (app rows
//! over shared ones, `pt-br` → `pt` → English), plurals, language names and locale matching are the
//! `craft-i18n` crate's (craft-libs).
//!
//! The `language` setting ([`crate::UiState::language`]) is [`AUTO`] by default: the UI follows the
//! system's preferred languages (macOS preferred languages, the Windows display language,
//! `LANGUAGE`/`LC_*`/`LANG` elsewhere, the browser's on the web) when there is a catalog for one of
//! them, and English otherwise. `DECKCRAFT_LANGUAGE=ja` overrides the system. Preferences ▸
//! Interface language, or the `app.language` command, picks one by hand. Tests always draw in
//! English ([`system_lang`]).
//!
//! # Writing interface text
//! - A fixed string: `tl!("Slide Sorter")` (or [`t`]). DeckCraft's own widgets
//!   ([`crate::widgets::big_button`], `small_button`, `icon_toggle`, `check_item`, `drop_button`,
//!   `section`, the ribbon's `big`/`small`/`toggle`/`menu_item`) translate the label they are given:
//!   pass them English.
//! - A string with runtime values: an English template with named placeholders, looked up first,
//!   then filled: `fmt(tl!("Slide {n} of {count}"), &[("n", &n.to_string()), ("count", &count.to_string())])`.
//!   Never `format!` the English and look the result up; never positional `{}` or format specs
//!   (`{:+}`, `{:.2}`) in a template: format the value first and pass it as a named argument.
//!   Translators may reorder the placeholders.
//! - A command's label: [`tid`]`(id, label)` (an `@id` row for the id, else the label's row).
//! - Status-bar and toast messages are stored in English and translated when painted, as
//!   [`tc`]`("@msg", text)`: an `@msg` row may give a whole runtime message its wording.
//!
//! # Adding a language
//! 1. craft-i18n gets `locales/<code>/deckcraft.tsv` and the language's `@id language.name` row.
//! 2. Add one row to [`LANGUAGES`].
//!
//! The Preferences dropdown and the system-language match pick it up from there.

use std::borrow::Cow;
use std::cell::Cell;
use std::sync::{LazyLock, OnceLock};

use craft_i18n::I18n;

pub use craft_i18n::fmt;

include!(concat!(env!("OUT_DIR"), "/craft_i18n.rs"));

/// The embedded catalogs, offering exactly the languages in [`LANGUAGES`]. Built on first use; a
/// locale is parsed on its first lookup.
static I18N: LazyLock<I18n> = LazyLock::new(|| {
    let offered: Vec<&str> = LANGUAGES.iter().map(|l| l.code).collect();
    let (i18n, problems) = I18n::new(CRAFT_I18N, &offered);
    problems.iter().for_each(|p| log::warn!("interface catalogs: {p}"));
    i18n
});

/// The `language` setting that follows the system.
pub const AUTO: &str = "auto";

/// The environment variable that overrides the system language (`ja`, `en`, `ja_JP.UTF-8`).
pub const LANGUAGE_ENV: &str = "DECKCRAFT_LANGUAGE";

/// One interface language DeckCraft offers. Its name and translations come from craft-i18n.
pub struct LangInfo {
    /// BCP 47 code, lowercase (`ja`, `zh-hans`). Also the `language` setting's value and the
    /// craft-i18n locale.
    pub code: &'static str,
}

/// The languages DeckCraft offers, when their catalogs are embedded. English first: it is the
/// fallback and the source language.
pub static LANGUAGES: [LangInfo; 2] = [
    LangInfo { code: "en" },
    // Japanese; `ja`, `ja-JP` and `ja_JP.UTF-8` resolve here.
    LangInfo { code: "ja" },
];

/// A language the UI can be shown in: English, or an offered language whose catalogs are embedded.
#[derive(Clone, Copy)]
pub struct Lang {
    lang: craft_i18n::Lang,
    /// The catalogs it is looked up in: the embedded ones ([`I18N`]), or a test fixture's.
    catalogs: Option<&'static I18n>,
    /// A test fixture's catalog text ([`message`] reads `@msg` templates from it); `None` reads
    /// [`CRAFT_I18N`].
    fixture: Option<&'static craft_i18n::Embedded>,
}

impl std::fmt::Debug for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lang({})", self.code())
    }
}

impl PartialEq for Lang {
    fn eq(&self, other: &Self) -> bool {
        self.code() == other.code()
    }
}

impl Eq for Lang {}

impl Lang {
    pub const EN: Lang = Lang { lang: craft_i18n::Lang::EN, catalogs: None, fixture: None };

    fn i18n(self) -> &'static I18n {
        self.catalogs.unwrap_or_else(|| LazyLock::force(&I18N))
    }

    /// Its BCP 47 code, lowercase (`en`, `ja`): the `language` setting's value.
    pub fn code(self) -> &'static str {
        self.i18n().code(self.lang)
    }

    /// The language's name in itself (its `@id language.name` row), shown in the Preferences
    /// dropdown.
    pub fn name(self) -> &'static str {
        self.i18n().name(self.lang)
    }

    /// Is the interface written in CJK script (its own name is), so it needs a CJK face?
    pub fn uses_cjk(self) -> bool {
        self.name().chars().any(is_cjk)
    }

    /// The embedded catalog text (shared, then DeckCraft's) this language reads; empty for English.
    /// The interface fonts look through it for characters they lack.
    pub fn catalog_text(self) -> String {
        let code = self.code();
        self.fixture
            .unwrap_or(CRAFT_I18N)
            .iter()
            .filter(|(c, ..)| c.eq_ignore_ascii_case(code))
            .flat_map(|(_, shared, app)| [*shared, *app])
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A language by its exact code (any case): one of [`LANGUAGES`] whose catalogs are embedded.
    pub fn from_code(code: &str) -> Option<Lang> {
        let info = LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(code))?;
        I18N.from_code(info.code).map(|lang| Lang { lang, catalogs: None, fixture: None })
    }

    /// Resolve the `language` setting: a language code, or [`AUTO`] (and anything unknown, e.g. a
    /// code from a newer version or one whose catalog isn't embedded) to follow the system.
    pub fn from_pref(pref: &str) -> Lang {
        Lang::from_code(pref).unwrap_or_else(system_lang)
    }

    /// Every language the interface can be shown in: [`LANGUAGES`] in order, those with embedded
    /// catalogs (English only in a build without craft-i18n).
    pub fn all() -> impl Iterator<Item = Lang> {
        LANGUAGES.iter().filter_map(|l| Lang::from_code(l.code))
    }

    /// Every language in the order a picker lists them: English first (the source language and
    /// the fallback), then the rest by their own name with accents folded. Names in other scripts
    /// follow the Latin ones in code point order.
    pub fn for_picker() -> Vec<Lang> {
        let mut rest: Vec<Lang> = Lang::all().filter(|l| l.code() != Lang::EN.code()).collect();
        rest.sort_by_cached_key(|l| name_sort_key(l.name()));
        std::iter::once(Lang::EN).chain(rest).collect()
    }
}

/// A CJK ideograph, kana or Hangul syllable (or CJK punctuation).
pub fn is_cjk(c: char) -> bool {
    matches!(c, '\u{2E80}'..='\u{9FFF}' | '\u{AC00}'..='\u{D7AF}' | '\u{F900}'..='\u{FAFF}' | '\u{FF00}'..='\u{FFEF}' | '\u{20000}'..='\u{3FFFF}')
}

/// The canonical `language` setting for user input: [`AUTO`] or a registered code (any case) whose
/// catalog is embedded. `None` for anything else, so callers keep the current setting.
pub fn normalize_pref(pref: &str) -> Option<&'static str> {
    if pref.eq_ignore_ascii_case(AUTO) {
        return Some(AUTO);
    }
    Lang::from_code(pref).map(Lang::code)
}

/// The registered language for a locale tag such as `ja_JP.UTF-8` or `ja-JP`; `None` when there is
/// no catalog for it. `C`/`POSIX` mean English.
pub fn lang_from_tag(tag: &str) -> Option<Lang> {
    I18N.match_tag(tag).and_then(|lang| Lang::from_code(I18N.code(lang)))
}

/// The first of the system's preferred languages DeckCraft has, in the user's order: someone who
/// prefers French, then Japanese, gets Japanese rather than English.
pub fn first_supported<'a>(tags: impl IntoIterator<Item = &'a str>) -> Option<Lang> {
    tags.into_iter().find_map(lang_from_tag)
}

/// The system language (looked up once). English when it can't be determined or isn't offered.
pub fn system_lang() -> Lang {
    // Tests drive the UI by its English labels whatever the machine's language is.
    if cfg!(test) {
        return Lang::EN;
    }
    static SYSTEM: OnceLock<Lang> = OnceLock::new();
    *SYSTEM.get_or_init(detect_system_lang)
}

fn detect_system_lang() -> Lang {
    // An explicit override, then the platform's preferred UI languages, read by the craft-i18n
    // crate through sys-locale (glibc's `@latin` modifier kept).
    if let Some(l) = std::env::var(LANGUAGE_ENV).ok().and_then(|v| lang_from_tag(&v)) {
        return l;
    }
    Lang::from_code(I18N.code(I18N.system_lang())).unwrap_or(Lang::EN)
}

thread_local! {
    // Per thread: parallel test harnesses must not change each other's language.
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::EN) };
}

/// Set the language the UI is drawn in (the app does this every frame from its setting).
pub fn set_current(lang: Lang) {
    CURRENT.set(lang);
}

/// The language the UI is drawn in.
pub fn current() -> Lang {
    CURRENT.get()
}

/// Does `lang` have a translation for this string? (English never does: it is the source.)
pub fn has(lang: Lang, s: &str) -> bool {
    lang.i18n().get(lang.lang, "", s).is_some()
}

/// `s` in the current language ([`tr`] with [`current`]); strings without a translation come back
/// unchanged. [`tl!`](crate::tl) is the short form.
pub fn t(s: &str) -> &str {
    tr(current(), s)
}

/// `s` in `lang`; strings without a translation come back unchanged.
pub fn tr(lang: Lang, s: &str) -> &str {
    #[cfg(test)]
    if pseudo::on() {
        return I18N.t(craft_i18n::Lang::PSEUDO, s);
    }
    lang.i18n().t(lang.lang, s)
}

/// `s` in the current language as worded for `context`, falling back to the plain translation and
/// then to English. `@msg` is the context of runtime messages (status bar, toasts): an `@msg` row
/// may give a whole message its wording.
pub fn tc<'a>(context: &str, s: &'a str) -> &'a str {
    trc(current(), context, s)
}

/// [`tc`] in a given language.
pub fn trc<'a>(lang: Lang, context: &str, s: &'a str) -> &'a str {
    #[cfg(test)]
    if pseudo::on() {
        return I18N.tc(craft_i18n::Lang::PSEUDO, context, s);
    }
    lang.i18n().tc(lang.lang, context, s)
}

/// `s`'s translation for `context` in the current language, with no fallback to the plain row or
/// English: for a word whose plain translation means something else here.
pub fn get<'a>(context: &str, s: &'a str) -> Option<&'a str> {
    #[cfg(test)]
    if pseudo::on() {
        return I18N.get(craft_i18n::Lang::PSEUDO, context, s);
    }
    let lang = current();
    lang.i18n().get(lang.lang, context, s)
}

/// A command's label in the current language: its `@id` row (keyed by the command id, for labels
/// whose English means different things), else the plain row of the English `label`. The label
/// stays English everywhere else (undo history, MCP, the CLI, the control channel).
pub fn tid<'a>(id: &str, label: &'a str) -> &'a str {
    trid(current(), id, label)
}

/// [`tid`] in a given language.
pub fn trid<'a>(lang: Lang, id: &str, label: &'a str) -> &'a str {
    #[cfg(test)]
    if pseudo::on() {
        return I18N.tid(craft_i18n::Lang::PSEUDO, id, label);
    }
    lang.i18n().tid(lang.lang, id, label)
}

/// A ribbon button's label in the current language. A two-line English label
/// (`"Selection\nPane"`) without a row of its own takes the one-line row (`Selection Pane`), which
/// the button then wraps itself ([`crate::widgets`]): the shared catalogs mostly hold one-line
/// wording.
pub fn label(s: &str) -> Cow<'_, str> {
    let direct = t(s);
    if direct != s || !s.contains('\n') {
        return Cow::Borrowed(direct);
    }
    let joined = s.replace('\n', " ");
    let one_line = t(&joined);
    if one_line == joined { Cow::Borrowed(s) } else { Cow::Owned(one_line.to_string()) }
}

/// A runtime message (status bar, toast) in the current language: its `@msg` row (or plain row),
/// else the first `@msg` template whose `{placeholders}` match the text there (`Couldn't open
/// {name}: {e}`), filled with what they matched, else the English. Messages are kept in English
/// (the control channel and MCP read them) and translated only when painted.
pub fn message(text: &str) -> Cow<'_, str> {
    let direct = tc("@msg", text);
    if direct != text {
        return Cow::Borrowed(direct);
    }
    let lang = current();
    for (template, translation) in msg_templates(lang) {
        if let Some(args) = match_template(&template, text) {
            let args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            return Cow::Owned(fmt(&translation, &args));
        }
    }
    Cow::Borrowed(text)
}

/// The `@msg` rows of `lang`'s catalogs (app catalog first, then the shared one; then the
/// language without its region) whose English has a `{placeholder}`: (English, translation).
fn msg_templates(lang: Lang) -> Vec<(String, String)> {
    let code = lang.code();
    if code == Lang::EN.code() {
        return Vec::new();
    }
    let embedded: &craft_i18n::Embedded = lang.fixture.unwrap_or(CRAFT_I18N);
    let base = code.rsplit_once('-').map(|(b, _)| b);
    let mut out = Vec::new();
    for want in std::iter::once(code).chain(base) {
        for (_, shared, app) in embedded.iter().filter(|(c, ..)| c.eq_ignore_ascii_case(want)) {
            for text in [*app, *shared] {
                for line in text.lines().filter_map(|l| l.strip_prefix("@msg\t")) {
                    let mut cols = line.trim_end_matches('\r').splitn(2, '\t');
                    if let (Some(en), Some(tr)) = (cols.next(), cols.next())
                        && en.contains('{')
                        && !tr.is_empty()
                    {
                        out.push((unescape(en), unescape(tr)));
                    }
                }
            }
        }
    }
    out
}

/// A catalog column's text: `\n`, `\t` and `\\` unescaped.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The placeholder values when `text` reads as `template` (`Couldn't open {name}: {e}`): each
/// placeholder takes the shortest non-empty text up to the literal after it, the last one the rest.
/// `None` when it doesn't match, or the template has no literal text or two placeholders in a row.
fn match_template(template: &str, text: &str) -> Option<Vec<(String, String)>> {
    // Literal, then (placeholder, literal) pairs.
    let (first, rest) = template.split_once('{').map_or((template, ""), |(a, b)| (a, b));
    let mut parts: Vec<(&str, &str)> = Vec::new();
    let mut rest = rest;
    while !rest.is_empty() {
        let (name, after) = rest.split_once('}')?;
        let (literal, next) = after.split_once('{').map_or((after, ""), |(a, b)| (a, b));
        parts.push((name, literal));
        rest = next;
    }
    if parts.is_empty() || (first.is_empty() && parts.iter().all(|(_, l)| l.is_empty())) {
        return None;
    }
    let mut at = text.strip_prefix(first)?;
    let mut args = Vec::new();
    for (k, (name, literal)) in parts.iter().enumerate() {
        let last = k + 1 == parts.len();
        let value = if literal.is_empty() {
            // Two placeholders in a row can't be told apart; only the last may end the text.
            if !last {
                return None;
            }
            std::mem::take(&mut at)
        } else if last {
            let v = at.strip_suffix(literal)?;
            at = "";
            v
        } else {
            // At least one character, then the first occurrence of the literal.
            let skip = at.chars().next()?.len_utf8();
            let i = at.get(skip..).and_then(|s| s.find(literal)).map(|i| i + skip)?;
            let v = at.get(..i)?;
            at = at.get(i + literal.len()..)?;
            v
        };
        if value.is_empty() {
            return None;
        }
        args.push(((*name).to_string(), value.to_string()));
    }
    at.is_empty().then_some(args)
}

/// The prompt an empty placeholder shows on the editing canvas, in the current language
/// (`deckcraft_render::RenderOpts::prompt`): the renderer can't reach the interface's catalogs.
pub fn placeholder_prompt(kind: deckcraft_model::PhType) -> &'static str {
    t(kind.prompt())
}

/// A test language whose DeckCraft catalog is `source` (catalog format), so tests of lookup code
/// don't depend on the embedded catalogs' wording.
#[cfg(test)]
pub(crate) fn fixture_lang(code: &'static str, source: &'static str) -> Lang {
    let embedded: &'static craft_i18n::Embedded = Box::leak(Box::new([(code, "", source)]));
    let (i18n, _) = I18n::new(embedded, &[code]);
    let i18n: &'static I18n = Box::leak(Box::new(i18n));
    let lang = i18n.from_code(code).unwrap_or(craft_i18n::Lang::EN);
    Lang { lang, catalogs: Some(i18n), fixture: Some(embedded) }
}

/// For a test that needs the embedded catalogs: `false`, with a note, when none were embedded
/// (`CRAFT_I18N_DIR` unset, or no `deckcraft.tsv` yet), so the test can skip; the interface is
/// then English only.
#[cfg(test)]
pub(crate) fn catalogs_embedded() -> bool {
    if CRAFT_I18N.is_empty() {
        eprintln!("skipped: built without craft-i18n catalogs (CRAFT_I18N_DIR); the interface is English only");
    }
    !CRAFT_I18N.is_empty()
}

#[cfg(test)]
mod audit;
#[cfg(test)]
pub(crate) mod pseudo;
#[cfg(test)]
mod tests;

/// `name` lowercased with the Latin accents that occur in language names folded away, so a
/// byte-wise sort reads alphabetically. Other scripts are left as they are.
fn name_sort_key(name: &str) -> String {
    name.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'è'..='ë' | 'ē' | 'ě' | 'ę' => 'e',
            'ì'..='ï' | 'ī' => 'i',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò'..='ö' | 'ø' | 'ő' => 'o',
            'ś' | 'š' => 's',
            'ù'..='ü' | 'ū' | 'ů' | 'ű' => 'u',
            'ý' | 'ÿ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            c => c,
        })
        .collect()
}
