use openidea_plugin::views::{Body, Button, ListItem, Table, TableSort, View, Viewport};
use openidea_plugin::{NotificationAction, NotificationLevel, StatusItem, StatusMenuEntry, host};

use crate::state::{StatEntry, Store};

pub const SELF_PREFIX: &str = "plugin.xirf.key-promoter-x.";
pub const HITLIST_VIEW: &str = "hitlist";
pub const SUPPRESSED_VIEW: &str = "suppressed";
pub const STATUS_ID: &str = "status";
pub const UPSTREAM_URL: &str = "https://github.com/halirutan/IntelliJ-Key-Promoter-X";

pub fn pluralize(count: u32) -> String {
    if count == 1 {
        "1 time".to_owned()
    } else {
        format!("{count} times")
    }
}

/// Refreshes both tool-window tabs and updates the status bar item & menu.
pub fn refresh_all_ui(store: &Store) {
    host::refresh_view(HITLIST_VIEW);
    host::refresh_view(SUPPRESSED_VIEW);
    update_status_bar(store);
}

/// Updates the global status bar indicator and its quick-action menu.
pub fn update_status_bar(store: &Store) {
    let (actions, missed, hits) = store.totals();
    let text = if store.snoozed {
        "KPX: Snoozed".to_owned()
    } else if missed > 0 {
        format!("KPX: {missed}")
    } else {
        "KPX".to_owned()
    };

    let state_label = if store.snoozed { "Snoozed" } else { "Active" };
    let tooltip = format!(
        "Key Promoter X ({state_label}) — {missed} missed across {actions} actions, {hits} keyboard hits"
    );

    let item = StatusItem {
        text,
        tooltip: Some(tooltip),
        command: Some("open-hitlist".to_owned()),
    };

    let snooze_label = if store.snoozed {
        "Resume Notifications"
    } else {
        "Snooze Notifications"
    };

    let mut menu = vec![
        StatusMenuEntry::Header("Key Promoter X".to_owned()),
        StatusMenuEntry::action("Show Missed Shortcuts", "open-hitlist")
            .with_description(format!("{actions} actions · {missed} missed")),
        StatusMenuEntry::action("Show Suppressed Shortcuts", "open-suppressed")
            .with_description(format!("{} suppressed", store.suppressed.len())),
        StatusMenuEntry::action(snooze_label, "toggle-snooze"),
    ];

    if let Some(last) = &store.last_action {
        menu.push(
            StatusMenuEntry::action("Suppress Last Action", "suppress-last")
                .with_description(last.clone()),
        );
    }

    menu.push(StatusMenuEntry::Separator);
    menu.push(StatusMenuEntry::action("Reset Statistics…", "reset"));
    menu.push(StatusMenuEntry::action("About & Credits", "about"));

    if host::set_status_item_menu(STATUS_ID, &item, &menu).is_err() {
        host::set_status_item(STATUS_ID, &item);
    }
}

/// Shows a shortcut reminder balloon (or blocking confirmation dialog in Hard Mode).
pub fn show_missed_shortcut_tip(entry: &StatEntry, hard_mode: bool) {
    let shortcut = entry.shortcut.as_deref().unwrap_or("");
    let times = pluralize(entry.missed);

    if hard_mode {
        let title = format!("Key Promoter X — {}", entry.title);
        let prompt = format!(
            "Command \"{}\" missed {times}.\nPress {shortcut} next time.",
            entry.title
        );
        if host::confirm(&title, &prompt).is_ok() {
            return;
        }
    }

    let title = format!("{} ({shortcut})", entry.title);
    let body = if entry.hits > 0 {
        format!(
            "Shortcut {shortcut} missed {times} (used {} via keyboard).",
            pluralize(entry.hits)
        )
    } else {
        format!("Shortcut {shortcut} missed {times}.")
    };

    let actions = [
        NotificationAction::new("Suppress Alert", "suppress").argument(&entry.action_id),
        NotificationAction::new("Show Hitlist", "open-hitlist"),
        NotificationAction::new("Snooze", "toggle-snooze"),
    ];

    if host::notify_with_actions(NotificationLevel::Info, &title, &body, &actions).is_err() {
        host::notify(NotificationLevel::Info, &title, &body);
    }
}

/// Suggests assigning a shortcut for a frequently mouse-clicked action without one.
pub fn show_create_shortcut_suggestion(action_id: &str, title: &str, count: u32) {
    let body = format!(
        "Want to create a shortcut for \"{title}\"? You clicked it {} without a shortcut. Assign one in Settings › Keymap.",
        pluralize(count)
    );
    let actions = [
        NotificationAction::new("Suppress Alert", "suppress").argument(action_id),
        NotificationAction::new("Snooze", "toggle-snooze"),
    ];

    if host::notify_with_actions(NotificationLevel::Info, "Key Promoter X", &body, &actions).is_err() {
        host::notify(NotificationLevel::Info, "Key Promoter X", &body);
    }
}

/// Renders the Missed Shortcuts sortable table view.
pub fn render_hitlist(store: &Store, sort: TableSort, viewport: Option<Viewport>) -> View {
    let snooze_label = if store.snoozed { "Resume" } else { "Snooze" };
    let toolbar = vec![
        Button::new("toggle-snooze", snooze_label),
        Button::new("reset", "Reset").icon("remove"),
        Button::new("refresh", "Refresh").icon("refresh"),
    ];

    let entries = store.sorted_hitlist(sort);
    if entries.is_empty() {
        let msg = if store.snoozed {
            "No missed shortcuts recorded yet.\nNotifications are currently snoozed."
        } else {
            "No missed shortcuts recorded yet.\nUse a menu or toolbar action with the mouse to see its keyboard shortcut here."
        };
        return View {
            toolbar,
            body: Body::Message(msg.to_owned()),
            status: Some("0 actions · Click Snooze to pause tips".to_owned()),
        };
    }

    let total = entries.len() as u32;
    let range = Viewport::range(viewport, total, 200);
    let first_row = range.start;
    let rows = entries[range.start as usize..range.end as usize]
        .iter()
        .map(|item| {
            vec![
                item.title.clone(),
                item.shortcut.clone().unwrap_or_default(),
                item.missed.to_string(),
                item.hits.to_string(),
            ]
        })
        .collect();

    let table = Table {
        columns: vec![
            "Action".to_owned(),
            "Shortcut".to_owned(),
            "Missed".to_owned(),
            "Hits".to_owned(),
        ],
        total_rows: total,
        first_row,
        rows,
        editable: false,
    };

    let (actions, missed, hits) = store.totals();
    let prefix = if store.snoozed { "Snoozed · " } else { "" };
    let status = format!(
        "{prefix}{actions} actions · {missed} missed · {hits} hits · Double-click a row to suppress"
    );

    View {
        toolbar,
        body: Body::sortable_table(&table, Some(sort)),
        status: Some(status),
    }
}

/// Renders the Suppressed Shortcuts list view.
pub fn render_suppressed(store: &Store) -> View {
    let snooze_label = if store.snoozed { "Resume" } else { "Snooze" };
    let toolbar = vec![
        Button::new("toggle-snooze", snooze_label),
        Button::new("unsuppress-all", "Restore All").enabled(!store.suppressed.is_empty()),
        Button::new("refresh", "Refresh").icon("refresh"),
    ];

    let items = store.sorted_suppressed();
    if items.is_empty() {
        return View {
            toolbar,
            body: Body::Message(
                "No suppressed shortcuts.\nClick \"Suppress Alert\" on any notification or double-click an entry in Missed Shortcuts."
                    .to_owned(),
            ),
            status: Some("0 suppressed".to_owned()),
        };
    }

    let list_items = items
        .iter()
        .map(|item| {
            let shortcut = item.shortcut.as_deref().unwrap_or("No shortcut");
            let detail = format!("{shortcut} · missed {}", pluralize(item.missed));
            ListItem::new(&format!("suppressed:{}", item.action_id), &item.title).detail(&detail)
        })
        .collect();

    let count = items.len();
    View {
        toolbar,
        body: Body::List(list_items),
        status: Some(format!("{count} suppressed · Double-click to reactivate")),
    }
}
