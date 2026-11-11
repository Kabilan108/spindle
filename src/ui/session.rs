//! What shows before the library: connecting, signing in, and the offline banner.

use rune::{Align, El, Tx, col, icon, row, text};

use super::theme::{Palette, font};
use super::widgets::pill_button;
use super::{Main, icons};
use crate::model::Command;

pub fn connecting(p: Palette, connecting: bool) -> El {
    col()
        .key("connecting")
        .w_full()
        .h_full()
        .center()
        .gap(14.0)
        .child(icon(&icons::MARK, 34.0).color(p.faint))
        .child(text(if connecting { "Connecting…" } else { "Starting…" }).font(font::BODY).color(p.muted))
}

pub fn sign_in(p: Palette, url: Option<&str>, error: Option<&str>) -> El {
    let mut card = col()
        .key("sign-in")
        .w(420.0)
        .gap(14.0)
        .items_center()
        .child(icon(&icons::MARK, 40.0).color(p.accent).mb(6.0))
        .child(text("Sign in to Baliset").font(font::PAGE_TITLE.lh(32.0)).color(p.text))
        .child(
            text("Baliset plays music from your Spotify Premium account. Signing in opens your browser.")
                .font(font::BODY)
                .color(p.muted)
                .align(Align::Center)
                .wrap(),
        )
        .child(
            pill_button("sign-in-button", if url.is_some() { "Open the browser again" } else { "Sign in" }, None, true, p)
                .mt(6.0)
                .on_press(|s: &mut Main| s.link.send(Command::Login)),
        );
    if let Some(url) = url {
        let copy = url.to_owned();
        card = card.child(
            row()
                .key("sign-in-url")
                .mt(4.0)
                .gap(10.0)
                .center_y()
                .child(text("Browser didn't open?").font(font::SMALL).color(p.muted))
                .child(
                    pill_button("copy-url", "Copy sign-in link", Some(&icons::COPY), false, p)
                        .on_press_tx(move |_: &mut Main, tx: &mut Tx| tx.copy(copy.clone())),
                ),
        );
    }
    if let Some(error) = error {
        card = card.child(text(error.to_owned()).key("login-error").font(font::SMALL).color(p.danger).align(Align::Center).wrap());
    }
    col().key("sign-in-screen").w_full().h_full().center().child(card)
}

pub fn offline_banner(p: Palette, error: &str) -> El {
    let message = if error.is_empty() { "Offline. Reconnecting…".to_owned() } else { format!("Offline: {error}. Reconnecting…") };
    row()
        .key("offline")
        .w_full()
        .h(28.0)
        .px(14.0)
        .gap(8.0)
        .center()
        .fill(p.warn_bg)
        .role("status")
        .child(icon(&icons::OFFLINE, 14.0).color(p.warn_text))
        .child(text(message).font(font::SMALL).color(p.warn_text).truncate().shrink())
}
