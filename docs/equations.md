# Equations

How DeckCraft inserts, stores, draws and exports equations, and where it stands against
PowerPoint. The code is `crates/math` (TeX, OMML, layout; no fonts of its own), the inline
object in `crates/text` (`MathObject`, `emit_math`), `crates/engine/src/cmd/equation.rs` and the
Equation dialog in `crates/ui-egui/src/dialogs.rs` (`equation_dialog`).

## Inserting: Insert ▸ Equation

The dialog takes TeX. The equation is drawn below the text as you type; while an edit is
half-typed (an unknown command, a missing brace) the last good drawing stays dimmed and the error
is named with its character position. Insert (⌘↩ / Ctrl+Enter) puts the equation at the insertion
point, replacing the selection, or, with no text being edited, in a text box of its own centred on
the slide (named "Equation", the paragraph centred, the run in Cambria Math). Opened with the
caret in or beside an equation, or with one selected, the dialog edits that equation: its TeX is
shown and the equation is selected, so Insert replaces it.

**Display style** (the dialog's switch) sets the equation as TeX's `\displaystyle` or a displayed
equation does: large operators with their limits above and below, fraction parts at full size
with TeX's display shifts (numerator up 0.677 em, denominator down 0.686 em about the axis at
0.25 em). Off, the equation is set in text style as in a line of text: limits beside the
operator, fraction parts at 70 % with the tighter shifts (0.394 / 0.345 em) — what `\frac` looks
like inside a paragraph in LaTeX. The switch is on for an equation in a box of its own and off at
an insertion point in text, and follows the equation when one is reopened. The file keeps the
choice the way Word does: a display equation is an `m:oMathPara`, an inline one a bare `m:oMath`;
`insert.equation {display?}` and `equation.at`'s `display` carry it on the control channel.

The keyboard stays in the TeX editor while the dialog is open (the Display style switch, a Recent
row or Clear hand it back after a click), with a steady caret in the accent colour.

Every equation inserted is remembered (latest first, no repeats, twenty kept) in the dialog's
Recent list, which is saved with the interface state (`ui.json`, `UiState::equations`); a row
puts that TeX back in the editor, Clear forgets them.

The same from the control channel or MCP: `insert.equation {tex}`, `equation.at {}` (the
equation the caret touches, as TeX with its character range) and `equation.check {tex}`
(parses, answers `{ok, text}` or the error's position and message). `app.dialog {id:
"equation", tex?}` and `ui.dialog.open {id: "equation", fields: {tex}}` open the dialog seeded.

## Sizing and editing an equation box

The text box Insert ▸ Equation makes hugs its equation (no wrapping, 2 pt insets so the caret
beside the equation stays inside the frame), so its handles sit on the equation. Dragging a
handle scales the equation itself — its font size follows the dragged side's ratio, measured on
the equation inside the box (the insets never scale), corner and side handles alike keep its
proportions — and the equation is laid out afresh at the new size, so it stays sharp at any size
(nothing is stretched as a picture). Shape Format ▸ Size, the Size and Position dialog and
`shape.resize {w | h}` scale it the same way; Home ▸ Font Size does too. A box with words beside
the equation is an ordinary text box and sizes as one.

Double-clicking an equation box opens the Equation dialog on its TeX, as does Insert ▸ Equation
while the box is selected (click an empty part of the slide first to insert a new equation
instead); Insert replaces the equation and leaves the box selected. `equation.at` names a selected
box's equation with its `id`.

## What the TeX covers

| Kind | TeX |
|---|---|
| Fractions | `\frac`, `\dfrac`, `\tfrac`, `\cfrac`, `{a \over b}`, `\binom`, `{n \choose k}`, `{a \atop b}` |
| Scripts | `x^2`, `x_i`, `x_i^2`, `{ab}^2`, primes `f'`, `f''`; a script binds to the last character of a run (`ab^2` is a b²) |
| Roots | `\sqrt{x}`, `\sqrt[n]{x}` |
| Large operators | `\sum`, `\prod`, `\coprod`, `\int`, `\iint`, `\iiint`, `\oint`, `\bigcup`, `\bigcap`, `\bigvee`, `\bigwedge`, `\bigoplus`, `\bigotimes`…, with `_` and `^` limits in either order, `\limits` / `\nolimits`; the operator applies to what follows up to the next relation, binary operator or punctuation |
| Delimiters | `\left( … \right)`, `\left\{`, `\left[`, `\left\|`, `\left\langle`, `\left\lfloor`, `\left.` (none), `\middle\|`; `\big(`… draw the plain character |
| Functions | `\sin`, `\cos`, `\tan`, `\log`, `\ln`, `\exp`, `\det`, `\gcd`, `\Pr`… (`\sin^2 x` scripts the name); `\lim`, `\max`, `\min`, `\sup`, `\inf`, `\limsup`, `\liminf`, `\argmax`… with their limits under or over |
| Accents and lines | `\hat`, `\widehat`, `\tilde`, `\bar`, `\vec`, `\dot`, `\ddot`, `\check`, `\breve`, `\acute`, `\grave`, `\overrightarrow`; `\overline`, `\underline`, `\overbrace{…}^{…}`, `\underbrace{…}_{…}`, `\overset`, `\underset`, `\stackrel`, `\boxed` |
| Matrices and lines | `\begin{matrix\|pmatrix\|bmatrix\|Bmatrix\|vmatrix\|Vmatrix\|smallmatrix\|array}` with `&` and `\\`; `\begin{cases}`; `\begin{aligned\|align\|gathered\|split\|eqnarray}`; `\\` and `&` at the top level make aligned lines |
| Text and styles | `\text{…}` (spaces kept), `\mathrm`, `\operatorname`, `\mathbf`, `\mathit`, `\boldsymbol`, `\mathbb`, `\mathcal`, `\mathfrak`, `\mathsf`, `\mathtt` (as Mathematical Alphanumeric Symbols) |
| Symbols | Greek (`\alpha`… `\Omega`, the `\var…` forms), operators (`\pm`, `\times`, `\cdot`, `\cup`…), relations (`\leq`, `\neq`, `\approx`, `\equiv`, `\in`, `\subset`…), arrows (`\to`, `\Rightarrow`, `\iff`, `\mapsto`…), `\infty`, `\partial`, `\nabla`, `\forall`, `\exists`, `\emptyset`, `\hbar`, `\ell`, `\dots`, `\cdots`, `\vdots`, `\ddots`, `\not=` and `\not\in`; Unicode typed directly (α, ∑, ≤) works too |
| Spaces | `\,`, `\:`, `\;`, `\!`, `\ `, `~`, `\quad`, `\qquad`, `\hspace{2em}` |
| Ignored | `\displaystyle`, `\textstyle`, `\label{}`, `\tag{}`, `\color{}`, `\nonumber`, `\mathstrut`, `%` comments |

Anything else is an error that names the command (`unknown command \foo (at character 5)`).
`-` is drawn as the minus sign, `*` as ∗. Input longer than 20,000 characters or nested more than
64 deep is refused.

## Storage and the file

The model keeps an equation as a run (`RunKind::Math { omml }`) whose text is the equation's
linear form (`x=(-b±√(b^2-4ac))/2a`: what Find searches and readers without equation support
see). The OMML is ECMA-376 §22.1: `m:oMathPara` › `m:oMath` with `m:f`, `m:sSup`, `m:sSub`,
`m:sSubSup`, `m:rad`, `m:nary`, `m:d`, `m:acc`, `m:bar`, `m:func`, `m:limLow`, `m:limUpp`, `m:m`,
`m:eqArr`, `m:groupChr`, `m:borderBox` and `m:r` (styles `m:sty` and `m:nor`). The PPTX writer
puts it in `a14:m` inside `mc:AlternateContent`, with the plain run as the fallback, exactly as
before; the reader keeps imported OMML untouched, so an equation from PowerPoint round-trips
byte for byte unless it is edited, and when it is edited it is read into the same tree
(`m:sPre` prescripts, `m:box` and `m:phant` are approximated; the properties it does not model are
dropped). The TeX shown when editing is regenerated from the tree (`tex::to_tex`), so an imported
equation opens as readable TeX.

Every OMML run carries `<a:rPr><a:latin typeface="Cambria Math"/></a:rPr>`, as PowerPoint writes
its own (its math font); the fallback run names it too. On import the first `a:rPr` inside the
equation gives the run its font, size and colour, and the run's text is the linear form of the
OMML read (what Find searches).

### The font

Cambria Math is what the file says (PowerPoint's math font, so the deck opens right there); what
DeckCraft draws is TeX's: letters, digits and Greek in **CMU Serif** (Computer Modern Unicode
0.7.0, OFL-1.1, the Computer Modern faces as OpenType — Roman, Italic, Bold, Bold Italic), the
operators and relations it lacks (∑ ∫ ≤ ∞ ⟨ …) in **STIX Two Math** (OFL-1.1), both bundled
through craft-fonts' Latin manifest. That holds even where Cambria Math is installed
(`MathFaces::new`); the font box shows Cambria Math. Another family picked in the font box draws
the equation in that family, its missing symbols from a math font (`MATH_SYMBOL_FONTS`: STIX Two
Math, Latin Modern Math, Cambria Math, XITS, Libertinus, Noto Sans Math, as installed) before the
general fallback. Latin Modern Math would be TeX's own math font in one file, but its GUST Font
License is not in the asset policy's list (AGENTS.md §1.2), so it is only used when the machine
has it. A build without craft-fonts draws Cambria Math as Caladea.

## Drawing

Layout follows The TeXbook's appendix G in outline (`crates/math/src/layout.rs`): atoms spaced
by class (thin/medium/thick spaces only in display and text style; a binary operator at the
start of a row or beside a relation is ordinary), scripts shifted by the base's height and depth
and set at 70 % then 50 %, fractions about the math axis (0.25 em) with 3× (display) or 1× (text)
the rule thickness of clearance, radicals with a clearance under the overbar and the sign stroked
as a path, large operators 1.4× in display style with limits above and below (integrals beside),
delimiters stretched by scaling the glyph's outline vertically about the axis once the content is
taller than the glyph, matrices with centred (or left-aligned) columns centred on the axis,
equation arrays aligned at their `&`. Glyph metrics come from the real faces: the run's family
in regular, bold, italic and bold italic, and for characters it lacks the database's fallback
(on macOS STIX Two Math and Apple Symbols are found; the bundled Liberation Serif covers most
operators).

In the text layout an equation is one cluster: its first character carries the equation's width
and draws it (glyph runs by face and size, bars as decorations, strokes as `TextLayout::paths`),
the others have no width, and a caret stands only before and after it. The line grows to the
equation's height. Display or text style comes from the equation itself (`m:oMathPara` or
`m:oMath`, the dialog's Display style switch), not from where it sits. The renderer, the editor canvas (which renders through it), the PDF writer
(`paths` filled) and the dialog preview (`render_equation`) draw the same layout; warped WordArt
text and Morph skip the paths.

## Not done, and what was not observed

- PowerPoint's own equation editor (the Equation tab's structure gallery, typing linear format
  or ink in place, the professional/linear toggle) is not reproduced: the dialog is the editor.
- PowerPoint for Mac's rendering of the OMML DeckCraft writes was not observed on this machine;
  the OMML follows the standard and the structure of files PowerPoint writes (`a14:m` in
  `mc:AlternateContent`, Cambria Math on every run).
- Layout constants are TeX's (Computer Modern), not read from a MATH table; sizes and spacing
  are close to PowerPoint's, not measured against it. Large operators are the text-size glyph
  scaled (no display-size variants).
- Equation text is not editable character by character on the slide (Backspace removes the whole
  equation); the dialog edits it.
- The linear form uses a few of UnicodeMath's characters (▒, ■, ¦) so the fallback text stays
  readable; it is not a UnicodeMath encoder, and UnicodeMath input is not parsed.

Checked by `cargo xtask ci`: `crates/math/src/tests.rs` (parsing, errors, no panics on random
input, OMML and TeX round trips, layout geometry with a mock font), the text layout tests
(`an_equation_run_is_one_cluster…`), `render::tests::equation_preview…`, the engine's
`insert_equation_*` and the dialog's `equation_dialog_tests`.
