//! Canned podcasts and follows: three shows (one a serial without dates), saved episodes and
//! followed artists.

use crate::model::{ContextKind, ContextPage, ContextSummary, Day, Details, EPISODES_URI, EpisodeProgress, NamedRef, Track};

struct Episode {
    title: &'static str,
    minutes: u32,
    released: Option<(i32, u32, u32)>,
    about: &'static str,
}

const fn ep(title: &'static str, minutes: u32, released: Option<(i32, u32, u32)>, about: &'static str) -> Episode {
    Episode { title, minutes, released, about }
}

struct Show {
    name: &'static str,
    publisher: &'static str,
    about: &'static str,
    /// Newest first, or in reading order for a serial.
    episodes: &'static [Episode],
}

const SHOWS: &[Show] = &[
    Show {
        name: "Signal & Noise",
        publisher: "Harbour Audio",
        about: "Two old radio engineers argue about the machines we live with, what they get right, and what they quietly break. New episodes every Thursday.",
        episodes: &[
            ep("The Last Modem in Lagos", 74, Some((2026, 11, 6)), "A dial-up line that never went down, the shop that kept it alive, and what twenty-five years of uptime teaches."),
            ep("Why Every Microwave Beeps", 41, Some((2026, 10, 30)), "Appliance sounds, who designs them, and the committee that decided what 'done' should sound like."),
            ep("Batteries, Honestly", 132, Some((2026, 10, 23)), "A long one: chemistry, recycling, and a listener who rebuilt a scooter pack on a kitchen table."),
            ep("Ask Us Anything: Antennas", 58, Some((2026, 10, 16)), "Your questions on rabbit ears, roof masts and why the radio works better when you touch it."),
            ep("The Quietest Room in Europe", 36, Some((2025, 12, 4)), "We visit an anechoic chamber and last eleven minutes before asking to leave."),
            ep("Pilot: Static", 29, Some((2025, 3, 2)), "Where the show began: a broken shortwave set and a bet about who could fix it first."),
        ],
    },
    Show {
        name: "The Slow Kitchen",
        publisher: "Ines Calder",
        about: "Recipes that take all afternoon and are worth it. Each episode is one dish, cooked in real time, with the mistakes left in.",
        episodes: &[
            ep("Jollof for Twelve", 96, Some((2026, 11, 9)), "Party rice in the big pot, the smoky bottom layer, and the argument about bay leaves."),
            ep("Bread That Waits Overnight", 63, Some((2026, 10, 26)), "A cold-proofed loaf, a sleepy baker, and why the fridge is the most patient tool you own."),
            ep("Stock, From Bones", 48, Some((2026, 10, 12)), "Roast, simmer, skim, repeat. The least glamorous episode and the one listeners write about most."),
            ep("Marmalade Season", 55, Some((2026, 1, 18)), "Seville oranges, a long boil, and the wrinkle test."),
        ],
    },
    Show {
        name: "The Lighthouse Ledger",
        publisher: "Marlowe & The Estuary",
        about: "A serial in nine parts. A keeper's logbook turns up at auction, and every entry after 1911 is in a different hand.",
        episodes: &[
            ep("Part One: Lot 214", 38, None, "The ledger is bought for less than the frame it came in."),
            ep("Part Two: The Second Hand", 42, None, "Someone else kept the light for thirty years, and nobody wrote down who."),
            ep("Part Three: Fog Signals", 45, None, "A record of every horn blast in the winter of 1913, and one that wasn't answered."),
            ep("Part Four: The Relief Boat", 40, None, "Supplies came every fortnight. One fortnight they didn't."),
        ],
    },
];

/// Saved episodes, newest saved first, as (show, episode).
const SAVED: &[(usize, usize)] = &[(1, 0), (0, 2), (2, 0), (0, 4), (1, 3)];

/// Indexes into the demo's albums, whose artists the account follows.
pub(super) const FOLLOWED_ARTISTS: &[usize] = &[3, 0, 5, 2];

fn show_uri(index: usize) -> String {
    format!("spotify:show:demo{index}")
}

fn show_image(index: usize) -> String {
    format!("demo://cover/{}", 40 + index * 3)
}

pub(super) fn show_summary(index: usize) -> ContextSummary {
    let show = &SHOWS[index];
    ContextSummary { uri: show_uri(index), name: show.name.into(), subtitle: show.publisher.into(), image_url: Some(show_image(index)), kind: ContextKind::Show }
}

pub(super) fn shows() -> Vec<ContextSummary> {
    (0..SHOWS.len()).map(show_summary).collect()
}

fn episode_uri(show: usize, episode: usize) -> String {
    format!("spotify:episode:demo{show}x{episode}")
}

fn episode_track(show: usize, episode: usize) -> Track {
    let named = NamedRef { uri: show_uri(show), name: SHOWS[show].name.into() };
    Track {
        uri: episode_uri(show, episode),
        name: SHOWS[show].episodes[episode].title.into(),
        artists: vec![named.clone()],
        album: Some(named),
        duration_ms: SHOWS[show].episodes[episode].minutes * 60_000 + 17_000,
        image_url: Some(show_image(show)),
        explicit: false,
        playable: true,
    }
}

fn show_tracks(show: usize) -> Vec<Track> {
    (0..SHOWS[show].episodes.len()).map(|episode| episode_track(show, episode)).collect()
}

pub(super) fn episodes_summary() -> ContextSummary {
    ContextSummary { uri: EPISODES_URI.into(), name: "Your Episodes".into(), subtitle: String::new(), image_url: None, kind: ContextKind::Episodes }
}

/// Every episode, for playing loose URIs.
pub(super) fn all_episodes() -> Vec<Track> {
    (0..SHOWS.len()).flat_map(show_tracks).collect()
}

pub(super) fn page(uri: &str) -> Option<ContextPage> {
    if uri == EPISODES_URI {
        let tracks = SAVED.iter().map(|&(show, episode)| episode_track(show, episode)).collect();
        return Some(ContextPage { summary: episodes_summary(), tracks, albums: Vec::new(), truncated: false });
    }
    let index = uri.strip_prefix("spotify:show:demo")?.parse::<usize>().ok().filter(|i| *i < SHOWS.len())?;
    Some(ContextPage { summary: show_summary(index), tracks: show_tracks(index), albums: Vec::new(), truncated: false })
}

pub(super) fn details() -> Vec<(String, Details)> {
    let mut all = Vec::new();
    for (s, show) in SHOWS.iter().enumerate() {
        all.push((show_uri(s), Details { description: show.about.into(), released: None }));
        for (e, episode) in show.episodes.iter().enumerate() {
            let released = episode.released.map(|(year, month, day)| Day { year, month, day });
            all.push((episode_uri(s, e), Details { description: episode.about.into(), released }));
        }
    }
    all
}

/// One episode heard to the end, one left part-way through.
pub(super) fn progress() -> Vec<(String, EpisodeProgress)> {
    vec![
        (episode_uri(0, 1), EpisodeProgress { position_ms: episode_track(0, 1).duration_ms, played: true }),
        (episode_uri(0, 2), EpisodeProgress { position_ms: 51 * 60_000, played: false }),
        (episode_uri(1, 0), EpisodeProgress { position_ms: 22 * 60_000, played: false }),
    ]
}
