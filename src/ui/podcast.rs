//! Podcasts: a show's page and Your Episodes. A header, then episodes as virtual rows (a
//! daily show runs to thousands), each with its date, length, a line of its description and
//! how far Baliset got into it.

use std::borrow::Cow;
use std::time::{SystemTime, UNIX_EPOCH};

use rune::{Cursor, Cx, El, GrowthSpec, KeyEvent, ScrollSpec, Tx, col, el, icon, row, text};

use super::page::list_key;
use super::theme::{Palette, font};
use super::widgets::{art, empty_state, pill_button};
use super::{Main, icons};
use crate::model::{Command, ContextKind, ContextSummary, Day, Details, EPISODES_URI, EpisodeProgress, Loadable, Store, Track};

const PAD: f32 = 20.0;
const ART: f32 = 176.0;
const ROW_ART: f32 = 56.0;
pub const EPISODE_ROW_H: f32 = 80.0;
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The pages this module draws instead of the track-list page.
pub fn draws(uri: &str) -> bool {
    uri == EPISODES_URI || uri.starts_with("spotify:show:")
}

/// The year of a count of days since 1970-01-01, by the proleptic Gregorian calendar.
fn year_of(days: i64) -> i32 {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let year = yoe + era * 400 + i64::from(mp >= 10);
    i32::try_from(year).unwrap_or(i32::MAX)
}

fn this_year() -> i32 {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    year_of(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// "Nov 6" this year, "Nov 6, 2025" before.
fn released(day: Day, this_year: i32) -> String {
    let month = MONTHS.get(day.month.saturating_sub(1) as usize).copied().unwrap_or("");
    if day.year == this_year { format!("{month} {}", day.day) } else { format!("{month} {}, {}", day.day, day.year) }
}

/// "41 min", "1 hr 6 min", "2 hr".
fn spoken_length(ms: u32) -> String {
    let minutes = (ms + 30_000) / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{} min", m.max(1)),
        (h, 0) => format!("{h} hr"),
        (h, m) => format!("{h} hr {m} min"),
    }
}

/// The part of the meta line that says how much of the episode is left.
fn remaining(duration_ms: u32, progress: Option<EpisodeProgress>) -> String {
    match progress {
        Some(EpisodeProgress { played: true, .. }) => "Played".to_owned(),
        Some(EpisodeProgress { position_ms, .. }) if position_ms < duration_ms => format!("{} left", spoken_length(duration_ms - position_ms)),
        _ => spoken_length(duration_ms),
    }
}

fn episodes_summary() -> ContextSummary {
    ContextSummary { uri: EPISODES_URI.into(), name: "Your Episodes".into(), subtitle: String::new(), image_url: None, kind: ContextKind::Episodes }
}

/// What the page is before its episodes arrive.
fn known_summary(store: &Store, uri: &str) -> ContextSummary {
    if uri == EPISODES_URI {
        return episodes_summary();
    }
    let lists = [store.shows.ready(), Some(&store.recents)];
    lists
        .into_iter()
        .flatten()
        .flat_map(|list| list.iter())
        .find(|item| item.uri == uri)
        .cloned()
        .unwrap_or_else(|| ContextSummary { uri: uri.into(), kind: ContextKind::Show, ..ContextSummary::default() })
}

enum State {
    Loading,
    Failed(String),
    Ready { count: usize, truncated: bool },
}

struct Facts {
    summary: ContextSummary,
    description: String,
    state: State,
    /// `None` on Your Episodes, which can't be followed.
    followed: Option<bool>,
    playing_here: bool,
}

fn facts(store: &Store, uri: &str) -> Facts {
    let (summary, state) = match store.pages.get(uri) {
        Some(Loadable::Ready(page)) => (page.summary.clone(), State::Ready { count: page.tracks.len(), truncated: page.truncated }),
        Some(Loadable::Failed(error)) => (known_summary(store, uri), State::Failed(error.clone())),
        _ => (known_summary(store, uri), State::Loading),
    };
    let playback = &store.playback;
    Facts {
        description: store.details.get(uri).map(|d| d.description.clone()).unwrap_or_default(),
        followed: (uri != EPISODES_URI).then(|| store.follows(uri)),
        playing_here: playback.playing && playback.context_uri.as_deref() == Some(uri),
        summary,
        state,
    }
}

pub fn view(s: &Main, _cx: &mut Cx<Main>, p: Palette, uri: &str) -> El {
    let facts = s.link.read(|store| facts(store, uri));
    let saved = uri == EPISODES_URI;
    let mut content = col().key("page").w_full().pb(28.0).child(header(s, p, &facts));
    content = match facts.state {
        State::Loading => content.child(col().px(PAD + 10.0).py(16.0).child(text("Loading…").font(font::SMALL).color(p.faint))),
        State::Failed(error) => {
            let retry_uri = uri.to_owned();
            content.child(empty_state("page-failed", &icons::OFFLINE, "Couldn't load this page", &error, p)).child(
                row().justify_center().child(pill_button("retry", "Try again", None, false, p).on_press(move |s: &mut Main| s.link.send(Command::LoadContext { uri: retry_uri.clone() }))),
            )
        }
        State::Ready { count: 0, .. } => {
            let body = if saved { "Episodes you save show up here." } else { "This show has no episodes yet." };
            content.child(empty_state("no-episodes", &icons::PODCAST, "No episodes", body, p))
        }
        State::Ready { count, truncated } => {
            content = content.child(episode_list(p, uri, count, saved));
            if truncated {
                content = content.child(text(format!("Showing the first {count} episodes.")).font(font::SMALL).color(p.faint).px(PAD + 10.0).pt(12.0));
            }
            content
        }
    };
    col().key(format!("page:{uri}")).w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).scroll_edge_fade(16.0).child(content)
}

fn header(s: &Main, p: Palette, f: &Facts) -> El {
    let summary = &f.summary;
    let saved = summary.kind == ContextKind::Episodes;
    let cover = if saved {
        el().size(ART, ART).radius(8.0).gradient(p.accent.mix(p.bg, 0.25), p.accent.mix(p.bg, 0.7)).center().child(icon(&icons::BOOKMARK_FILLED, 64.0).color(p.on_accent))
    } else {
        art(&s.art, &s.link, summary.image_url.as_deref(), ART, 8.0, p, &icons::PODCAST)
    };
    let mut info = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(6.0)
        .child(text(if saved { "COLLECTION" } else { "PODCAST" }).font(font::CAPS).color(p.muted))
        .child(text(summary.name.clone()).key("page-title").font(font::PAGE_TITLE).color(p.text).max_lines(2));
    if !summary.subtitle.is_empty() {
        info = info.child(text(summary.subtitle.clone()).key("publisher").font(font::NAV).color(p.text).truncate());
    }
    if let State::Ready { count, truncated } = f.state
        && count > 0
    {
        let plus = if truncated { "+" } else { "" };
        info = info.child(text(format!("{count}{plus} {}", if count == 1 { "episode" } else { "episodes" })).font(font::SMALL).color(p.muted));
    }
    let uri = summary.uri.clone();
    let (label, glyph) = if f.playing_here { ("Pause", &icons::PAUSE) } else { ("Play", &icons::PLAY) };
    let mut buttons = row().mt(10.0).gap(10.0).child(pill_button("page-play", label, Some(glyph), true, p).on_press(move |s: &mut Main| s.play_or_toggle(&uri)));
    if let Some(followed) = f.followed {
        let uri = summary.uri.clone();
        let (label, glyph) = if followed { ("Following", Some(&icons::CHECK)) } else { ("Follow", None) };
        buttons = buttons.child(pill_button("follow", label, glyph, false, p).checked(followed).on_press(move |s: &mut Main| s.link.send(Command::SetFollowed { uri: uri.clone(), followed: !followed })));
    }
    info = info.child(buttons);
    let top = row().key("page-header").px(PAD + 8.0).pt(28.0).pb(24.0).gap(24.0).items_end().child(cover.shadow(6.0, 20.0, p.shadow)).child(info);
    let mut out = col().key("show-header").w_full().child(top);
    if !f.description.is_empty() {
        out = out.child(text(f.description.clone()).key("show-about").font(font::BODY).color(p.muted).max_lines(3).max_w(760.0).px(PAD + 8.0).pb(20.0));
    }
    out
}

fn episode_list(p: Palette, uri: &str, count: usize, saved: bool) -> El {
    let (rows_uri, key_uri) = (uri.to_owned(), uri.to_owned());
    col()
        .key("episodes")
        .px(PAD)
        .role("list")
        .label("Episodes")
        .focusable(true)
        .on_key(move |s: &mut Main, ev: &KeyEvent, tx: &mut Tx| list_key(s, ev, tx, &key_uri, count))
        .child(el().h(1.0).w_full().fill(p.line))
        .child(
            col()
                .key("episode-rows")
                .growth(GrowthSpec::default())
                .virtual_rows(count, EPISODE_ROW_H, |_: &Main, i| Cow::Owned(format!("trk{i}")), move |s: &Main, i| episode_row(s, &rows_uri, i, saved))
                .virtual_rows_version((uri.to_owned(), count)),
        )
}

struct RowFacts {
    track: Track,
    details: Details,
    progress: Option<EpisodeProgress>,
    playing_this: bool,
    playing: bool,
    selected: bool,
}

fn row_facts(store: &Store, uri: &str, i: usize, selected: Option<usize>) -> Option<RowFacts> {
    let track = store.pages.get(uri)?.ready()?.tracks.get(i)?.clone();
    let playback = &store.playback;
    Some(RowFacts {
        details: store.details.get(&track.uri).cloned().unwrap_or_default(),
        progress: store.episode_progress.get(&track.uri).copied(),
        playing_this: playback.track.as_ref().is_some_and(|t| t.uri == track.uri),
        playing: playback.playing,
        selected: selected == Some(i),
        track,
    })
}

/// How far in, as a short bar under the meta line's end.
fn progress_bar(p: Palette, fraction: f32) -> El {
    const W: f32 = 44.0;
    el().w(W).h(3.0).radius(1.5).fill(p.line).child(el().absolute(0.0, 0.0).size((W * fraction.clamp(0.0, 1.0)).round(), 3.0).radius(1.5).fill(p.accent))
}

/// One episode: art, title, a line of its description, then show (on Your Episodes), date
/// and what's left. A double click or the round button plays it inside the page's context.
fn episode_row(s: &Main, uri: &str, i: usize, saved: bool) -> El {
    let p = Palette::new(s.dark.get());
    let Some(f) = s.link.read(|store| row_facts(store, uri, i, s.selected)) else {
        return el().h(EPISODE_ROW_H);
    };
    let RowFacts { track: t, details, progress, playing_this, playing, selected } = f;
    let show = t.album.as_ref().map(|show| show.name.clone()).unwrap_or_default();
    let mut parts = Vec::new();
    if saved && !show.is_empty() {
        parts.push(show.clone());
    }
    if let Some(day) = details.released {
        parts.push(released(day, this_year()));
    }
    parts.push(remaining(t.duration_ms, progress));
    let mut meta = row().gap(8.0).center_y().min_w(0.0);
    if progress.is_some_and(|progress| progress.played) {
        meta = meta.child(icon(&icons::CHECK, 12.0).color(p.accent));
    }
    meta = meta.child(text(parts.join("  ·  ")).font(font::SMALL).color(p.faint).truncate().shrink());
    if let Some(EpisodeProgress { position_ms, played: false }) = progress
        && t.duration_ms > 0
    {
        meta = meta.child(progress_bar(p, position_ms as f32 / t.duration_ms as f32));
    }
    let mut titles = col().grow(1.0).min_w(0.0).gap(3.0).child(text(t.name.clone()).font(font::ROW_TITLE).color(if playing_this { p.accent } else { p.text }).truncate());
    if !details.description.is_empty() {
        titles = titles.child(text(details.description.clone()).font(font::ROW_SUB).color(p.muted).truncate());
    }
    titles = titles.child(meta);

    let (context, track_uri) = (uri.to_owned(), t.uri.clone());
    let play = move |s: &mut Main| s.play_context(&context, Some(track_uri.clone()));
    let pause_here = playing_this && playing;
    let button = {
        let play = play.clone();
        el().key("row-play")
            .role("button")
            .label(if pause_here { "Pause" } else { "Play episode" })
            .size(36.0, 36.0)
            .radius(18.0)
            .center()
            .fill(if playing_this { p.accent } else { p.raised })
            .cursor(Cursor::Pointer)
            .hover(p.hover)
            .press(p.press)
            .on_press(move |s: &mut Main| if playing_this { s.link.send(Command::PlayPause) } else { play(s) })
            .child(icon(if pause_here { &icons::PAUSE } else { &icons::PLAY }, 15.0).color(if playing_this { p.on_accent } else { p.text }))
    };
    let r = row()
        .h(EPISODE_ROW_H)
        .px(10.0)
        .gap(14.0)
        .center_y()
        .radius(6.0)
        .fill(if selected { p.selected } else { rune::Color::TRANSPARENT })
        .hover(p.hover)
        .cursor(Cursor::Pointer)
        .role("row")
        .label(format!("{}, {show}", t.name))
        .checked(selected)
        .on_press(move |s: &mut Main| s.selected = Some(i))
        .child(art(&s.art, &s.link, t.image_url.as_deref(), ROW_ART, 6.0, p, &icons::PODCAST))
        .child(titles)
        .child(button);
    if t.playable { r.on_double_press(play) } else { r.opacity(0.45) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_since_the_epoch_land_in_the_right_year() {
        assert_eq!(year_of(0), 1970);
        assert_eq!(year_of(-1), 1969);
        assert_eq!(year_of(20_768), 2026, "2026-11-11");
        assert_eq!(year_of(20_453), 2025, "2025-12-31");
        assert_eq!(year_of(20_454), 2026, "2026-01-01");
    }

    #[test]
    fn a_release_day_names_its_year_only_when_it_isnt_this_one() {
        let day = Day { year: 2026, month: 11, day: 6 };
        assert_eq!(released(day, 2026), "Nov 6");
        assert_eq!(released(Day { year: 2025, month: 1, day: 30 }, 2026), "Jan 30, 2025");
    }

    #[test]
    fn lengths_read_the_way_people_say_them() {
        assert_eq!(spoken_length(10_000), "1 min");
        assert_eq!(spoken_length(41 * 60_000), "41 min");
        assert_eq!(spoken_length(3_991_579), "1 hr 7 min");
        assert_eq!(spoken_length(2 * 3_600_000), "2 hr");
        assert_eq!(spoken_length(10_947_603), "3 hr 2 min");
    }

    #[test]
    fn what_is_left_follows_how_far_baliset_got() {
        let hour = 3_600_000;
        assert_eq!(remaining(hour, None), "1 hr");
        assert_eq!(remaining(hour, Some(EpisodeProgress { position_ms: 45 * 60_000, played: false })), "15 min left");
        assert_eq!(remaining(hour, Some(EpisodeProgress { position_ms: hour, played: true })), "Played");
    }
}
