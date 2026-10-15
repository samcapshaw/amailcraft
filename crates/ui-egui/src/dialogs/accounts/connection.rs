//! Microsoft 365 setup presentation. Status describes observed state, never inferred server grants.
use super::*;

pub(super) fn graph_provider(st: &Wizard) -> &'static str {
    if st.graph_editing {
        "microsoft-graph-pim-write"
    } else if st.graph_calendars || st.graph_contacts {
        "microsoft-graph-pim"
    } else {
        "microsoft-graph"
    }
}

pub(super) fn valid_application_id(value: &str) -> bool {
    let value = value.trim();
    value.len() == 36
        && value
            .bytes()
            .enumerate()
            .all(|(i, b)| if matches!(i, 8 | 13 | 18 | 23) { b == b'-' } else { b.is_ascii_hexdigit() })
}

use super::setup::card;

pub(super) fn graph_settings(ui: &mut Ui, app: &mut App, st: &mut Wizard, uid: u64) {
    let account = st.change.or(st.created).and_then(|id| account_of(app, id));
    card(ui, app, "Your mailbox", |ui| {
        ui.label(RichText::new(st.address.trim()).strong());
        if let Some(link) = account.as_ref().and_then(|a| a.linked_mailbox.as_ref()) {
            muted(ui, app, &format!("Sign in as {}", link.principal_address));
            muted(
                ui,
                app,
                if link.send_as {
                    "Sending is enabled in MailCraft. Microsoft 365 still controls Send As and Send on Behalf access."
                } else {
                    "Receive only. Sending from this mailbox is disabled in MailCraft."
                },
            );
        }
    });
    card(ui, app, "Services", |ui| {
        ui.label(RichText::new("Mail").size(15.0));
        muted(ui, app, "Included with this account");
        ui.separator();
        let calendars = setup::service(
            ui,
            app,
            "Calendars",
            "Events and meeting invitations",
            &mut st.graph_calendars,
            true,
        );
        let contacts = setup::service(
            ui,
            app,
            "Contacts",
            "People in your default address book",
            &mut st.graph_contacts,
            true,
        );
        if !(st.graph_calendars || st.graph_contacts) {
            st.graph_editing = false;
        }
        ui.separator();
        let editing = setup::service(
            ui,
            app,
            "Editing and meeting responses",
            "Enable supported organizer changes",
            &mut st.graph_editing,
            st.graph_calendars || st.graph_contacts,
        );
        if calendars.changed() || contacts.changed() || editing.changed() {
            st.extra.providers.clear();
            st.extra.provider = 0;
        }
        ui.add_space(6.0);
        muted(
            ui,
            app,
            if st.graph_editing {
                "Organizer editing requested"
            } else if st.graph_calendars || st.graph_contacts {
                "Read-only organizer access"
            } else {
                "Mail access only"
            },
        );
        egui::CollapsingHeader::new("Sync details").show(ui, |ui| {
            muted(ui, app, "Calendars cover 90 days back and one year ahead. Contacts use the default address book. Editing depends on mailbox permissions. Changing services may require signing in again.");
        });
    });
    // The sign-in body dispatches commands and therefore needs mutable access to the app.
    let theme = app.theme.clone();
    egui::Frame::new()
        .fill(theme.card_surface())
        .stroke(egui::Stroke::new(0.5, theme.separator))
        .corner_radius(if theme.terminal_style { 4 } else { 10 })
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(RichText::new("Sign-in").font(theme.detail_font(13.0)).color(theme.text_muted));
            ui.add_space(8.0);
            wizard_status(ui, app, st);
            if !st.extra.client_id.trim().is_empty() && !valid_application_id(&st.extra.client_id) {
                muted(ui, app, "Enter the 36-character Application (client) ID from your registration, including its hyphens.");
            }
            if app.s.view.work_offline {
                muted(ui, app, "You're offline. Save these settings now, then turn off Work Offline to sign in.");
            }
            ui.hyperlink_to("Application registration and permission guide ↗", "https://github.com/samcapshaw/amailcraft/blob/master/docs/microsoft365.md");
            if let Some(url) = oauth_section(ui, app, st, uid) {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        });
}

pub(super) fn wizard_status(ui: &mut Ui, app: &App, st: &Wizard) {
    let Some(a) = st.change.or(st.created).and_then(|id| account_of(app, id)) else {
        muted(
            ui,
            app,
            "Not signed in yet. Your browser handles Microsoft sign-in; no account password is needed here.",
        );
        return;
    };
    let options = mailcraft_engine::accounts::account_options(&app.s, a.id);
    if options
        .oauth
        .as_ref()
        .is_some_and(|b| b.provider != graph_provider(st) || b.client_id.trim() != st.extra.client_id.trim())
    {
        muted(
            ui,
            app,
            "Sign in again to authorize the selected services or application. Existing credentials have not verified these changes.",
        );
    } else {
        account_status(ui, app, &a);
    }
}

pub(super) fn account_status(ui: &mut Ui, app: &App, a: &Account) {
    let status = app.s.sync_status().accounts.iter().find(|s| s.account == a.id.0);
    let has_sign_in = app.s.secrets.get(&mailcraft_engine::accounts::secret_key(a.id, false)).is_some()
        || app.s.secrets.get(&mailcraft_engine::accounts::refresh_key(a.id)).is_some();
    let title = if app.s.view.work_offline {
        "Offline · connection paused"
    } else if status.is_some_and(|s| s.busy) {
        "Connecting…"
    } else if status.is_some_and(|s| s.error.is_some()) {
        "Connection needs attention"
    } else if !has_sign_in {
        "Sign-in required"
    } else if status.is_some_and(|s| s.last_sync.is_some()) {
        "Last mailbox sync succeeded"
    } else {
        "Sign-in saved · mailbox access not yet verified"
    };
    ui.label(RichText::new(title).strong());
    if let Some(error) = status.and_then(|s| s.error.as_deref()) {
        muted(ui, app, recovery_hint(error));
        egui::CollapsingHeader::new("Technical details").show(ui, |ui| {
            ui.label(RichText::new(error).size(11.0).color(app.theme.text_muted));
        });
    }
    if status.is_some_and(|s| s.busy) {
        ctx_repaint(ui);
    }
}

pub(super) fn account_services(ui: &mut Ui, app: &App, a: &Account) {
    let options = mailcraft_engine::accounts::account_options(&app.s, a.id);
    let services = [
        "Mail",
        if options.graph_calendars { "Calendars on" } else { "Calendars off" },
        if options.graph_contacts { "Contacts on" } else { "Contacts off" },
        if options.graph_editing { "Editing requested" } else { "Read-only organizer" },
    ]
    .join(" · ");
    muted(ui, app, &services);
}

pub(super) fn recovery_hint(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("401") || lower.contains("invalid_grant") || lower.contains("expired") {
        "Sign in again, then test the connection. Your organization may require renewed consent."
    } else if lower.contains("403") || lower.contains("accessdenied") || lower.contains("access denied") {
        "Ask your administrator to check mailbox access and consent for the selected services. Enabling an option here does not grant access."
    } else if lower.contains("404") {
        "Check the mailbox address and whether your organization has provisioned this mailbox."
    } else if lower.contains("429") {
        "Microsoft 365 is limiting requests. Wait before trying again."
    } else {
        "Check your network connection and account settings, then test the connection. Details below can help identify the failing service."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_id_rejects_non_ascii_and_malformed_registrations() {
        assert!(valid_application_id(" 12345678-abcd-1234-ABCD-1234567890ab "));
        for value in [
            "",
            "my-app",
            "12345678abcd1234abcd1234567890abcd",
            "12345678-abcd-1234-ABCD-1234567890ag",
            "é2345678-abcd-1234-ABCD-1234567890ab",
        ] {
            assert!(!valid_application_id(value));
        }
    }

    #[test]
    fn consent_profile_tracks_services_and_editing() {
        let mut st = Wizard::default();
        assert_eq!(graph_provider(&st), "microsoft-graph");
        st.graph_contacts = true;
        assert_eq!(graph_provider(&st), "microsoft-graph-pim");
        st.graph_editing = true;
        assert_eq!(graph_provider(&st), "microsoft-graph-pim-write");
    }
}
