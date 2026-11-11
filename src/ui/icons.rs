//! Line icons on a 24-unit grid, drawn in the element's colour.

use rune::Icon;

const fn line(name: &'static str, body: &'static str) -> Icon {
    Icon { name, view: 24.0, body }
}

pub static HOME: Icon = line(
    "baliset-home",
    r#"<path d="M4 10.5 12 4l8 6.5V19a1 1 0 0 1-1 1h-4.5v-5.5h-5V20H5a1 1 0 0 1-1-1z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#,
);
pub static SEARCH: Icon = line(
    "baliset-search",
    r#"<circle cx="10.5" cy="10.5" r="6" fill="none" stroke="currentColor" stroke-width="1.8"/><path d="m15 15 5 5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#,
);
pub static HEART: Icon = line(
    "baliset-heart",
    r#"<path d="M12 19.5s-7.5-4.4-7.5-10A4.2 4.2 0 0 1 12 7a4.2 4.2 0 0 1 7.5 2.5c0 5.6-7.5 10-7.5 10z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#,
);
pub static HEART_FILLED: Icon = line(
    "baliset-heart-filled",
    r#"<path d="M12 19.5s-7.5-4.4-7.5-10A4.2 4.2 0 0 1 12 7a4.2 4.2 0 0 1 7.5 2.5c0 5.6-7.5 10-7.5 10z" fill="currentColor" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/>"#,
);
pub static PLAY: Icon = line("baliset-play", r#"<path d="M8 5.2v13.6a.8.8 0 0 0 1.2.7l11-6.8a.8.8 0 0 0 0-1.4l-11-6.8A.8.8 0 0 0 8 5.2z" fill="currentColor"/>"#);
pub static PAUSE: Icon = line(
    "baliset-pause",
    r#"<rect x="6" y="5" width="4" height="14" rx="1" fill="currentColor"/><rect x="14" y="5" width="4" height="14" rx="1" fill="currentColor"/>"#,
);
pub static NEXT: Icon = line(
    "baliset-next",
    r#"<path d="M5 6.2v11.6a.7.7 0 0 0 1.1.6l8.6-5.8a.7.7 0 0 0 0-1.2L6.1 5.6A.7.7 0 0 0 5 6.2z" fill="currentColor"/><rect x="16.5" y="5" width="2.5" height="14" rx="1" fill="currentColor"/>"#,
);
pub static PREVIOUS: Icon = line(
    "baliset-previous",
    r#"<path d="M19 6.2v11.6a.7.7 0 0 1-1.1.6l-8.6-5.8a.7.7 0 0 1 0-1.2l8.6-5.8a.7.7 0 0 1 1.1.6z" fill="currentColor"/><rect x="5" y="5" width="2.5" height="14" rx="1" fill="currentColor"/>"#,
);
pub static SHUFFLE: Icon = line(
    "baliset-shuffle",
    r#"<path d="M4 7h3.5c1.6 0 2.8.8 3.7 2.1l2.6 3.8c.9 1.3 2.1 2.1 3.7 2.1H20M4 17h3.5c1.4 0 2.4-.6 3.2-1.6M13.3 8.6C14.1 7.6 15.1 7 16.5 7H20M17.5 4.5 20 7l-2.5 2.5M17.5 14.5 20 17l-2.5 2.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub static REPEAT: Icon = line(
    "baliset-repeat",
    r#"<path d="M5 11V9.5A2.5 2.5 0 0 1 7.5 7H19M16.5 4.5 19 7l-2.5 2.5M19 13v1.5a2.5 2.5 0 0 1-2.5 2.5H5M7.5 19.5 5 17l2.5-2.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub static REPEAT_ONE: Icon = line(
    "baliset-repeat-one",
    r#"<path d="M5 11V9.5A2.5 2.5 0 0 1 7.5 7H19M16.5 4.5 19 7l-2.5 2.5M19 13v1.5a2.5 2.5 0 0 1-2.5 2.5H5M7.5 19.5 5 17l2.5-2.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/><path d="M11.2 10.6 12.5 9.8v5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub static VOLUME: Icon = line(
    "baliset-volume",
    r#"<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5z" fill="currentColor" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
);
pub static VOLUME_OFF: Icon = line(
    "baliset-volume-off",
    r#"<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5z" fill="currentColor" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="m15.5 9.5 5 5m0-5-5 5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
);
pub static CLOSE: Icon = line("baliset-close", r#"<path d="m6.5 6.5 11 11m0-11-11 11" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#);
pub static NOTE: Icon = line(
    "baliset-note",
    r#"<path d="M9.5 17.5V6.5l9-2v11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/><circle cx="7" cy="17.5" r="2.5" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="16" cy="15.5" r="2.5" fill="none" stroke="currentColor" stroke-width="1.6"/>"#,
);
pub static PERSON: Icon = line(
    "baliset-person",
    r#"<circle cx="12" cy="9" r="3.6" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M5 19.5c1.2-3.3 3.8-5 7-5s5.8 1.7 7 5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
);
pub static PLAYING: Icon = line(
    "baliset-playing",
    r#"<rect x="4.5" y="9" width="3" height="10" rx="1" fill="currentColor"/><rect x="10.5" y="5" width="3" height="14" rx="1" fill="currentColor"/><rect x="16.5" y="12" width="3" height="7" rx="1" fill="currentColor"/>"#,
);
pub static SIGN_OUT: Icon = line(
    "baliset-sign-out",
    r#"<path d="M10 4.5H6.5a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2H10M15 8l4 4-4 4M19 12H9.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>"#,
);
pub static COPY: Icon = line(
    "baliset-copy",
    r#"<rect x="8.5" y="8.5" width="11" height="11" rx="2" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M15.5 8.5V6.5a2 2 0 0 0-2-2h-7a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h2" fill="none" stroke="currentColor" stroke-width="1.6"/>"#,
);
pub static OFFLINE: Icon = line(
    "baliset-offline",
    r#"<path d="M3.5 9.5a12.5 12.5 0 0 1 17 0M6.5 12.7a8 8 0 0 1 11 0M9.5 15.8a3.6 3.6 0 0 1 5 0" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><path d="m4 4 16 16" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
);
pub static LIBRARY: Icon = line(
    "baliset-library",
    r#"<path d="M5 4.5v15M9.5 4.5v15M14 5.5l4.5 13.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>"#,
);
/// Baliset's mark: a zither frame with a sloping head and its strings.
pub static MARK: Icon = line(
    "baliset-mark",
    r#"<path d="M4.2 3.9V20h16M4.2 3.9l15.6 8.5" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/><path d="M7.1 6.4V17M10.1 8V17M13.1 9.6V17M16.1 11.2V17" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>"#,
);
pub static SETTINGS: Icon = line(
    "baliset-settings",
    r#"<path d="M4.5 7h9M18.5 7h1M4.5 12h2M11.5 12h8M4.5 17h7M16.5 17h3" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/><circle cx="16" cy="7" r="2.3" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="9" cy="12" r="2.3" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="14" cy="17" r="2.3" fill="none" stroke="currentColor" stroke-width="1.6"/>"#,
);
