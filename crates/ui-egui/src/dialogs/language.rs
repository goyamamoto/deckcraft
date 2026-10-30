use super::SlideApp;

/// The selection's proofing language and whether it's marked Do Not Check Spelling: the language
/// of the text at the caret (the first selected character, else the first run of the first
/// selected shape), inherited from its placeholder, master and the deck's default (`ja-JP` in a
/// Japanese deck) when the run names none, as the engine resolves it
/// ([`deckcraft_engine::cmd::format::selection_language`]); US English when nothing says.
pub fn selected_language(app: &SlideApp) -> (String, bool) {
    let (lang, no_proof) = deckcraft_engine::cmd::format::selection_language(&app.session);
    (lang.unwrap_or_else(|| "en-US".into()), no_proof)
}
