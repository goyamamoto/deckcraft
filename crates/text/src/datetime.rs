//! Date and time fields (`a:fld type="datetime…"`): the text each field type shows for a moment,
//! and the local clock.
//!
//! The field types and their formats follow ECMA-376 Part 1, §21.1.2.2.4 (`fld`): `datetime1`
//! to `datetime13`, written as in PowerPoint's English (United States) Date and Time list.
//! `datetime` and `datetimeFigureOut` show the language's short date. Numeric dates follow the
//! run's language. Formats with month or day names, or AM/PM, are computed for English and
//! Japanese; other languages keep the text the file was saved with.
//!
//! Japanese (`ja`, `ja-JP`) runs show each type as PowerPoint for Mac does (observed 2026-10-24
//! at 16:12:15, a Saturday): the slash date `2026/10/24` (datetime1, datetime5,
//! datetimeFigureOut), `2026年10月24日(土)` with the weekday (datetime2), `2026年10月24日`
//! (datetime4), the Japanese era (和暦) dates `令和8年10月24日` (datetime3), `令和8年10月`
//! (datetime6) and `令和八年十月二十四日` in kanji numerals (datetime7), the 24-hour clock with
//! 時/分/秒 (`16時12分`, datetime12 and 13, after a two-digit-year slash date in datetime8 and 9),
//! and `16:12` / `16:12:15` (datetime10, 11). Zero padding couldn't be seen on that date: the
//! slash dates pad month and day to two digits as the Japanese short date does (`2026/01/05`),
//! the 年月日 forms and the hour don't, and minutes and seconds always have two digits.

/// A local date and time: what the clock read, or a fixed moment (tests, deterministic output).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: i32,
    /// 1–12.
    pub month: u32,
    /// 1–31.
    pub day: u32,
    /// 0–23.
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl DateTime {
    pub fn new(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Self {
        DateTime { year, month, day, hour, minute, second }
    }

    /// The local date and time now (the user's time zone); `None` where there is no clock (wasm).
    pub fn now_local() -> Option<DateTime> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use chrono::{Datelike, Timelike};
            let n = chrono::Local::now();
            Some(DateTime::new(n.year(), n.month(), n.day(), n.hour(), n.minute(), n.second()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            None
        }
    }

    /// A real calendar date and time of day.
    fn is_valid(&self) -> bool {
        let date_ok = (1..=9999).contains(&self.year) && chrono::NaiveDate::from_ymd_opt(self.year, self.month, self.day).is_some();
        date_ok && self.hour < 24 && self.minute < 60 && self.second < 61
    }

    /// Day of the week, 0 = Sunday.
    fn weekday(&self) -> usize {
        match chrono::NaiveDate::from_ymd_opt(self.year, self.month, self.day) {
            // `chrono::Weekday::num_days_from_sunday` is 0 = Sunday.
            Some(d) => chrono::Datelike::weekday(&d).num_days_from_sunday() as usize,
            None => 0,
        }
    }
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
/// The Japanese weekdays' one-character names, Sunday first.
const JA_DAYS: [&str; 7] = ["日", "月", "火", "水", "木", "金", "土"];

/// An era of the Japanese calendar (和暦): its name and first day (year, month, day).
struct Era {
    name: &'static str,
    start: (i32, u32, u32),
}

/// The modern eras, oldest first. An era's first year is year 1: PowerPoint writes 令和8 for
/// 2026, counting 2019 as 令和1 (and so 一 in kanji; no sign of 元年). 明治 counts from the first
/// day of its first year (1868-01-25 in the Gregorian calendar), as it was applied back to then.
const ERAS: [Era; 5] = [
    Era { name: "明治", start: (1868, 1, 25) },
    Era { name: "大正", start: (1912, 7, 30) },
    Era { name: "昭和", start: (1926, 12, 25) },
    Era { name: "平成", start: (1989, 1, 8) },
    Era { name: "令和", start: (2019, 5, 1) },
];

/// The Japanese era (和暦) a date falls in, and the year within it (1 for the era's first year);
/// `None` before 明治.
pub fn era_of(year: i32, month: u32, day: u32) -> Option<(&'static str, i32)> {
    let era = ERAS.iter().rev().find(|e| (year, month, day) >= e.start)?;
    Some((era.name, year.checked_sub(era.start.0)?.checked_add(1)?))
}

/// Kanji digits, 〇 for zero.
const KANJI_DIGITS: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];

fn kanji_digit(n: u32) -> &'static str {
    KANJI_DIGITS.get(n as usize).copied().unwrap_or("")
}

/// A number in kanji numerals, as a date is written in them: 八, 十, 十一, 二十, 二十四, 三十一,
/// 九十九, 百二十, 千八百六十八 (no 一 before 十, 百 or 千). Zero is 〇; from 10,000 on, digit by
/// digit.
pub fn kanji_number(n: u32) -> String {
    if n == 0 || n >= 10_000 {
        return kanji_digits(n);
    }
    let mut out = String::new();
    for (unit, size) in [("千", 1000), ("百", 100), ("十", 10)] {
        let q = (n / size) % 10;
        if q > 1 {
            out.push_str(kanji_digit(q));
        }
        if q > 0 {
            out.push_str(unit);
        }
    }
    let ones = n % 10;
    if ones > 0 {
        out.push_str(kanji_digit(ones));
    }
    out
}

/// A number digit by digit in kanji (一八五〇 for 1850): a Western year written in kanji.
fn kanji_digits(n: u32) -> String {
    n.to_string().chars().map(|c| kanji_digit(c.to_digit(10).unwrap_or(0))).collect()
}

/// The calendar a field type counts its years in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Calendar {
    Gregorian,
    /// The Japanese era (和暦): 令和8年 for 2026.
    JapaneseEra,
}

/// The calendar field `kind` counts years in for a run of language `lang`: in Japanese,
/// datetime3, datetime6 and datetime7 are era dates; every other type, and every type in other
/// languages, is Gregorian.
pub fn calendar_of(kind: &str, lang: Option<&str>) -> Calendar {
    let japanese = lang.is_some_and(|l| primary_language(&l.trim().to_ascii_lowercase()) == "ja");
    if japanese && matches!(kind, "datetime3" | "datetime6" | "datetime7") { Calendar::JapaneseEra } else { Calendar::Gregorian }
}

/// The primary language subtag of a lower-case BCP 47 tag (`ja` for `ja-jp`).
fn primary_language(lang: &str) -> &str {
    lang.split(['-', '_']).next().unwrap_or("")
}

/// Is field `kind` a date/time field?
pub fn is_date_field(kind: &str) -> bool {
    kind.starts_with("datetime")
}

/// The text date/time field `kind` shows at `now` in a run of language `lang` (BCP 47, e.g.
/// `en-US`). `None` when `kind` isn't a date/time type this knows, `now` isn't a real date, or
/// the format needs names in a language other than English or Japanese: the field keeps its
/// saved text.
pub fn field_text(kind: &str, lang: Option<&str>, now: DateTime) -> Option<String> {
    if !now.is_valid() {
        return None;
    }
    let lang = lang.map(str::trim).filter(|l| !l.is_empty()).unwrap_or("en-US").to_ascii_lowercase();
    if primary_language(&lang) == "ja" {
        return japanese_text(kind, now);
    }
    let english = lang == "en" || lang.starts_with("en-");
    let month = MONTHS.get(now.month.checked_sub(1)? as usize)?;
    let mon = month.get(..3)?;
    let day = DAYS.get(now.weekday())?;
    let (d, y) = (now.day, now.year);
    let yy = y.rem_euclid(100);
    let h12 = match now.hour % 12 {
        0 => 12,
        h => h,
    };
    let ampm = if now.hour < 12 { "AM" } else { "PM" };
    let (mi, s) = (now.minute, now.second);
    let short = short_date(&lang, now);
    let named = |text: String| english.then_some(text);
    match kind {
        "datetime" | "datetimeFigureOut" | "datetime1" => Some(short),
        "datetime2" => named(format!("{day}, {month} {d}, {y}")),
        "datetime3" => named(format!("{d} {month} {y}")),
        "datetime4" => named(format!("{month} {d}, {y}")),
        "datetime5" => named(format!("{d}-{mon}-{yy:02}")),
        "datetime6" => named(format!("{month} {yy:02}")),
        "datetime7" => named(format!("{mon}-{yy:02}")),
        "datetime8" => named(format!("{short} {h12}:{mi:02} {ampm}")),
        "datetime9" => named(format!("{short} {h12}:{mi:02}:{s:02} {ampm}")),
        "datetime10" => Some(format!("{}:{mi:02}", now.hour)),
        "datetime11" => Some(format!("{}:{mi:02}:{s:02}", now.hour)),
        "datetime12" => named(format!("{h12}:{mi:02} {ampm}")),
        "datetime13" => named(format!("{h12}:{mi:02}:{s:02} {ampm}")),
        _ => None,
    }
}

/// What field `kind` shows at `now` (a real date) in a Japanese run; see the module docs.
fn japanese_text(kind: &str, now: DateTime) -> Option<String> {
    let (y, m, d) = (now.year, now.month, now.day);
    let (h, mi, s) = (now.hour, now.minute, now.second);
    let yy = y.rem_euclid(100);
    let wd = JA_DAYS.get(now.weekday())?;
    // The era and its year; before 明治, the Western year.
    let era = era_of(y, m, d);
    let era_year = || match era {
        Some((name, n)) => format!("{name}{n}"),
        None => y.to_string(),
    };
    Some(match kind {
        "datetime" | "datetimeFigureOut" | "datetime1" | "datetime5" => short_date("ja", now),
        "datetime2" => format!("{y}年{m}月{d}日({wd})"),
        "datetime3" => format!("{}年{m}月{d}日", era_year()),
        "datetime4" => format!("{y}年{m}月{d}日"),
        "datetime6" => format!("{}年{m}月", era_year()),
        "datetime7" => {
            let year = match era {
                Some((name, n)) => format!("{name}{}", kanji_number(u32::try_from(n).ok()?)),
                None => kanji_digits(u32::try_from(y).ok()?),
            };
            format!("{year}年{}月{}日", kanji_number(m), kanji_number(d))
        }
        "datetime8" => format!("{yy:02}/{m:02}/{d:02} {h}時{mi:02}分"),
        "datetime9" => format!("{yy:02}/{m:02}/{d:02} {h}時{mi:02}分{s:02}秒"),
        "datetime10" => format!("{h}:{mi:02}"),
        "datetime11" => format!("{h}:{mi:02}:{s:02}"),
        "datetime12" => format!("{h}時{mi:02}分"),
        "datetime13" => format!("{h}時{mi:02}分{s:02}秒"),
        _ => return None,
    })
}

/// The short (numeric) date of language `lang` (lower case), US order unless the language is
/// known to write its dates another way. Japanese pads month and day to two digits
/// (`2026/01/05`), as its short date (`y/MM/dd`) does.
fn short_date(lang: &str, t: DateTime) -> String {
    let (y, m, d) = (t.year, t.month, t.day);
    let primary = primary_language(lang);
    match lang {
        "en-gb" | "en-au" | "en-nz" | "en-ie" | "en-in" => return format!("{d:02}/{m:02}/{y}"),
        "en-ca" | "fr-ca" | "en-za" => return format!("{y}-{m:02}-{d:02}"),
        _ => {}
    }
    match primary {
        "fr" | "es" | "it" | "pt" | "el" | "ca" | "vi" => format!("{d:02}/{m:02}/{y}"),
        "de" | "ru" | "pl" | "cs" | "sk" | "tr" | "fi" | "nb" | "nn" | "no" | "da" | "uk" | "ro" => format!("{d:02}.{m:02}.{y}"),
        "nl" => format!("{d}-{m}-{y}"),
        "ja" => format!("{y}/{m:02}/{d:02}"),
        "zh" => format!("{y}/{m}/{d}"),
        "ko" | "sv" | "lt" => format!("{y}-{m:02}-{d:02}"),
        _ => format!("{m}/{d}/{y}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRI: DateTime = DateTime { year: 2026, month: 10, day: 9, hour: 20, minute: 5, second: 7 };

    #[test]
    fn english_formats_of_every_type() {
        let f = |k: &str| field_text(k, Some("en-US"), FRI);
        assert_eq!(f("datetime1").as_deref(), Some("10/9/2026"));
        assert_eq!(f("datetime2").as_deref(), Some("Friday, October 9, 2026"));
        assert_eq!(f("datetime3").as_deref(), Some("9 October 2026"));
        assert_eq!(f("datetime4").as_deref(), Some("October 9, 2026"));
        assert_eq!(f("datetime5").as_deref(), Some("9-Oct-26"));
        assert_eq!(f("datetime6").as_deref(), Some("October 26"));
        assert_eq!(f("datetime7").as_deref(), Some("Oct-26"));
        assert_eq!(f("datetime8").as_deref(), Some("10/9/2026 8:05 PM"));
        assert_eq!(f("datetime9").as_deref(), Some("10/9/2026 8:05:07 PM"));
        assert_eq!(f("datetime10").as_deref(), Some("20:05"));
        assert_eq!(f("datetime11").as_deref(), Some("20:05:07"));
        assert_eq!(f("datetime12").as_deref(), Some("8:05 PM"));
        assert_eq!(f("datetime13").as_deref(), Some("8:05:07 PM"));
        assert_eq!(f("datetimeFigureOut").as_deref(), Some("10/9/2026"));
        assert_eq!(f("datetime").as_deref(), Some("10/9/2026"));
        // No language: US English. Midnight and noon are 12 on the clock.
        assert_eq!(field_text("datetime12", None, DateTime::new(2024, 2, 29, 0, 0, 0)).as_deref(), Some("12:00 AM"));
        assert_eq!(field_text("datetime12", None, DateTime::new(2024, 2, 29, 12, 30, 0)).as_deref(), Some("12:30 PM"));
        assert_eq!(field_text("datetime2", None, DateTime::new(2000, 1, 1, 0, 0, 0)).as_deref(), Some("Saturday, January 1, 2000"));
    }

    #[test]
    fn language_orders_numeric_dates_and_names_need_english_or_japanese() {
        assert_eq!(field_text("datetime1", Some("en-GB"), FRI).as_deref(), Some("09/10/2026"));
        assert_eq!(field_text("datetimeFigureOut", Some("de-DE"), FRI).as_deref(), Some("09.10.2026"));
        // The Japanese short date pads month and day (y/MM/dd), so the 9th is 09.
        assert_eq!(field_text("datetime1", Some("ja-JP"), FRI).as_deref(), Some("2026/10/09"));
        assert_eq!(field_text("datetime1", Some("sv-SE"), FRI).as_deref(), Some("2026-10-09"));
        assert_eq!(field_text("datetime1", Some("zh-CN"), FRI).as_deref(), Some("2026/10/9"));
        assert_eq!(field_text("datetime10", Some("fr-FR"), FRI).as_deref(), Some("20:05"));
        // Month names in another language: not computed (the field keeps its saved text).
        assert_eq!(field_text("datetime2", Some("de-DE"), FRI), None);
        assert_eq!(field_text("datetime12", Some("fr-FR"), FRI), None);
        assert_eq!(field_text("datetime2", Some("zh-CN"), FRI), None);
        assert_eq!(field_text("datetime3", Some("zh-TW"), FRI), None);
        assert_eq!(field_text("datetime12", Some("ko-KR"), FRI), None);
        assert_eq!(field_text("datetime4", Some("en-GB"), FRI).as_deref(), Some("October 9, 2026"));
    }

    /// Saturday 2026-10-24 16:12:15, when PowerPoint for Mac's ja-JP fields were read.
    const SAT: DateTime = DateTime { year: 2026, month: 10, day: 24, hour: 16, minute: 12, second: 15 };

    #[test]
    fn japanese_formats_of_every_type_match_powerpoint() {
        let f = |k: &str| field_text(k, Some("ja-JP"), SAT);
        assert_eq!(f("datetime1").as_deref(), Some("2026/10/24"));
        assert_eq!(f("datetime2").as_deref(), Some("2026年10月24日(土)"));
        assert_eq!(f("datetime3").as_deref(), Some("令和8年10月24日"));
        assert_eq!(f("datetime4").as_deref(), Some("2026年10月24日"));
        assert_eq!(f("datetime5").as_deref(), Some("2026/10/24"));
        assert_eq!(f("datetime6").as_deref(), Some("令和8年10月"));
        assert_eq!(f("datetime7").as_deref(), Some("令和八年十月二十四日"));
        assert_eq!(f("datetime8").as_deref(), Some("26/10/24 16時12分"));
        assert_eq!(f("datetime9").as_deref(), Some("26/10/24 16時12分15秒"));
        assert_eq!(f("datetime10").as_deref(), Some("16:12"));
        assert_eq!(f("datetime11").as_deref(), Some("16:12:15"));
        assert_eq!(f("datetime12").as_deref(), Some("16時12分"));
        assert_eq!(f("datetime13").as_deref(), Some("16時12分15秒"));
        assert_eq!(f("datetimeFigureOut").as_deref(), Some("2026/10/24"));
        assert_eq!(f("datetime").as_deref(), Some("2026/10/24"));
        assert_eq!(f("datetime14"), None);
        // Any spelling of the Japanese tag.
        for tag in ["ja", "JA-jp", "ja_JP", " ja-JP "] {
            assert_eq!(field_text("datetime2", Some(tag), SAT).as_deref(), Some("2026年10月24日(土)"), "{tag}");
        }
    }

    #[test]
    fn japanese_slash_dates_pad_the_month_and_day_and_times_pad_minutes() {
        // Monday 2026-01-05 09:03:07.
        let f = |k: &str| field_text(k, Some("ja-JP"), DateTime::new(2026, 1, 5, 9, 3, 7));
        assert_eq!(f("datetime1").as_deref(), Some("2026/01/05"));
        assert_eq!(f("datetime2").as_deref(), Some("2026年1月5日(月)"));
        assert_eq!(f("datetime3").as_deref(), Some("令和8年1月5日"));
        assert_eq!(f("datetime4").as_deref(), Some("2026年1月5日"));
        assert_eq!(f("datetime5").as_deref(), Some("2026/01/05"));
        assert_eq!(f("datetime6").as_deref(), Some("令和8年1月"));
        assert_eq!(f("datetime7").as_deref(), Some("令和八年一月五日"));
        assert_eq!(f("datetime8").as_deref(), Some("26/01/05 9時03分"));
        assert_eq!(f("datetime9").as_deref(), Some("26/01/05 9時03分07秒"));
        assert_eq!(f("datetime10").as_deref(), Some("9:03"));
        assert_eq!(f("datetime11").as_deref(), Some("9:03:07"));
        assert_eq!(f("datetime12").as_deref(), Some("9時03分"));
        assert_eq!(f("datetime13").as_deref(), Some("9時03分07秒"));
        assert_eq!(f("datetimeFigureOut").as_deref(), Some("2026/01/05"));
        // Midnight is hour 0 on the 24-hour clock; a two-digit year keeps its zero.
        let midnight = DateTime::new(2005, 12, 31, 0, 0, 0);
        assert_eq!(field_text("datetime9", Some("ja-JP"), midnight).as_deref(), Some("05/12/31 0時00分00秒"));
        assert_eq!(field_text("datetime2", Some("ja-JP"), midnight).as_deref(), Some("2005年12月31日(土)"));
        assert_eq!(field_text("datetime7", Some("ja-JP"), midnight).as_deref(), Some("平成十七年十二月三十一日"));
    }

    #[test]
    fn japanese_era_dates_change_era_on_its_first_day() {
        let era = |y, m, d| {
            let t = DateTime::new(y, m, d, 12, 0, 0);
            (field_text("datetime3", Some("ja-JP"), t).unwrap_or_default(), field_text("datetime7", Some("ja-JP"), t).unwrap_or_default())
        };
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(era(2019, 4, 30), pair("平成31年4月30日", "平成三十一年四月三十日"));
        assert_eq!(era(2019, 5, 1), pair("令和1年5月1日", "令和一年五月一日"));
        assert_eq!(era(1989, 1, 7), pair("昭和64年1月7日", "昭和六十四年一月七日"));
        assert_eq!(era(1989, 1, 8), pair("平成1年1月8日", "平成一年一月八日"));
        assert_eq!(era(1926, 12, 24), pair("大正15年12月24日", "大正十五年十二月二十四日"));
        assert_eq!(era(1926, 12, 25), pair("昭和1年12月25日", "昭和一年十二月二十五日"));
        assert_eq!(era(1912, 7, 29), pair("明治45年7月29日", "明治四十五年七月二十九日"));
        assert_eq!(era(1912, 7, 30), pair("大正1年7月30日", "大正一年七月三十日"));
        assert_eq!(era(1868, 1, 25), pair("明治1年1月25日", "明治一年一月二十五日"));
        // Before 明治: the Western year (in kanji digits in the kanji form).
        assert_eq!(era(1868, 1, 24), pair("1868年1月24日", "一八六八年一月二十四日"));
        assert_eq!(era(1, 1, 1), pair("1年1月1日", "一年一月一日"));
        // Era years past 99 still read as numbers.
        assert_eq!(era(2118, 5, 1), pair("令和100年5月1日", "令和百年五月一日"));
        assert_eq!(field_text("datetime6", Some("ja-JP"), DateTime::new(2019, 4, 30, 0, 0, 0)).as_deref(), Some("平成31年4月"));
        assert_eq!(field_text("datetime6", Some("ja-JP"), DateTime::new(2019, 5, 1, 0, 0, 0)).as_deref(), Some("令和1年5月"));
        assert_eq!(field_text("datetime2", Some("ja-JP"), DateTime::new(2019, 5, 1, 0, 0, 0)).as_deref(), Some("2019年5月1日(水)"));
        assert_eq!(era_of(2026, 10, 24), Some(("令和", 8)));
        assert_eq!(era_of(1868, 1, 24), None);
        assert_eq!(era_of(i32::MIN, 1, 1), None);
        assert_eq!(era_of(i32::MAX, u32::MAX, u32::MAX).map(|(e, _)| e), Some("令和"));
        // An impossible date is still nothing in Japanese.
        assert_eq!(field_text("datetime7", Some("ja-JP"), DateTime::new(2019, 2, 30, 0, 0, 0)), None);
    }

    #[test]
    fn kanji_numbers_are_positional() {
        let k = kanji_number;
        assert_eq!(k(0), "〇");
        assert_eq!(k(1), "一");
        assert_eq!(k(8), "八");
        assert_eq!(k(10), "十");
        assert_eq!(k(11), "十一");
        assert_eq!(k(20), "二十");
        assert_eq!(k(24), "二十四");
        assert_eq!(k(31), "三十一");
        assert_eq!(k(99), "九十九");
        assert_eq!(k(100), "百");
        assert_eq!(k(120), "百二十");
        assert_eq!(k(1868), "千八百六十八");
        assert_eq!(k(2026), "二千二十六");
        assert_eq!(k(9999), "九千九百九十九");
        assert_eq!(k(10_000), "一〇〇〇〇");
        assert_eq!(k(u32::MAX).chars().count(), 10);
    }

    #[test]
    fn only_japanese_era_types_count_years_by_era() {
        assert_eq!(calendar_of("datetime3", Some("ja-JP")), Calendar::JapaneseEra);
        assert_eq!(calendar_of("datetime6", Some("ja")), Calendar::JapaneseEra);
        assert_eq!(calendar_of("datetime7", Some("JA_jp")), Calendar::JapaneseEra);
        assert_eq!(calendar_of("datetime1", Some("ja-JP")), Calendar::Gregorian);
        assert_eq!(calendar_of("datetime2", Some("ja-JP")), Calendar::Gregorian);
        assert_eq!(calendar_of("datetime3", Some("en-US")), Calendar::Gregorian);
        assert_eq!(calendar_of("datetime3", None), Calendar::Gregorian);
    }

    #[test]
    fn unknown_types_and_impossible_dates_give_nothing() {
        assert_eq!(field_text("slidenum", None, FRI), None);
        assert_eq!(field_text("datetime14", None, FRI), None);
        for bad in [
            DateTime::new(2026, 0, 9, 0, 0, 0),
            DateTime::new(2026, 13, 9, 0, 0, 0),
            DateTime::new(2026, 2, 29, 0, 0, 0),
            DateTime::new(2026, 4, 31, 0, 0, 0),
            DateTime::new(2026, 1, 1, 24, 0, 0),
            DateTime::new(i32::MIN, 1, 1, 0, 0, 0),
            DateTime::new(i32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX),
        ] {
            assert_eq!(field_text("datetime2", None, bad), None, "{bad:?}");
        }
        assert!(is_date_field("datetimeFigureOut") && !is_date_field("slidenum"));
    }

    #[test]
    fn the_clock_reads_a_real_date() {
        if let Some(now) = DateTime::now_local() {
            assert!(field_text("datetime1", None, now).is_some(), "{now:?}");
        }
    }
}
