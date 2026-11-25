//! Key Promoter X for OpenIDEA.
//!
//! Ported from IntelliJ Key Promoter X by Patrick Scheibe (`halirutan`),
//! originally created by Dmitry Kashin and maintained by Athiele.
//! Licensed under the BSD 3-Clause License (see `LICENSE` and `NOTICE.md`).

mod settings;
mod state;
mod ui;

use openidea_plugin::views::{TableSort, View, ViewEvent, Viewport, Views};
use openidea_plugin::{
    ActionSource, Context, Error, Event, EventKind, Invocation, NotificationAction,
    NotificationLevel, PickItem, Plugin, host, register_plugin,
};

use crate::settings::Settings;
use crate::state::{StatEntry, Store};

pub struct KeyPromoterX {
    hitlist_sort: TableSort,
}

impl Default for KeyPromoterX {
    fn default() -> Self {
        Self {
            // Default sort: Missed count descending, matching Key Promoter X.
            hitlist_sort: TableSort { column: 2, descending: true },
        }
    }
}

enum PendingTip {
    Missed(StatEntry),
    SuggestCreate {
        action_id: String,
        title: String,
        count: u32,
    },
}

/// IDE actions dispatched automatically/programmatically by OpenIDEA with `ActionSource::Other`
/// (e.g., disk sync on window focus runs `project.refresh` and `vcs.refresh`; internal editor
/// hints run `editor.showHint`). Explicit invocations from Menu/Toolbar/ContextMenu/Search are
/// still tracked normally.
const AUTOMATIC_OTHER_ACTIONS: &[&str] = &[
    "project.refresh",
    "vcs.refresh",
    "editor.showHint",
];

fn is_automatic_other_action(action_id: &str, source: ActionSource) -> bool {
    source == ActionSource::Other && AUTOMATIC_OTHER_ACTIONS.contains(&action_id)
}

impl Plugin for KeyPromoterX {
    fn activate(&mut self, _ctx: &Context) -> Result<(), Error> {
        if let Err(err) = host::subscribe(EventKind::ActionPerformed)
            && !matches!(err, Error::Denied(_))
        {
            return Err(err);
        }
        let _ = host::subscribe(EventKind::SettingsChanged);
        if let Ok(store) = Store::load() {
            ui::update_status_bar(&store);
        }
        Ok(())
    }

    fn handle_events(&mut self, events: &[Event], _ctx: &Invocation) {
        let Ok(mut store) = Store::load() else {
            return;
        };
        let mut settings = Settings::load();
        let mut changed = false;
        let mut pending_tip: Option<PendingTip> = None;

        for event in events {
            match event {
                Event::Other { kind, .. } if kind == "settings-changed" => {
                    settings = Settings::load();
                    changed = true;
                }
                Event::ActionPerformed(action) => {
                    if action.action.is_empty() || action.action.starts_with(ui::SELF_PREFIX) {
                        continue;
                    }
                    if store.snoozed || store.is_suppressed(&action.action) {
                        continue;
                    }
                    if action.source == ActionSource::Keyboard {
                        if store.record_keyboard_hit(&action.action).unwrap_or(false) {
                            changed = true;
                        }
                        continue;
                    }
                    if !settings.is_source_enabled(action.source) {
                        continue;
                    }
                    if is_automatic_other_action(&action.action, action.source) {
                        continue;
                    }

                    let title = if action.title.trim().is_empty() {
                        action.action.as_str()
                    } else {
                        action.title.as_str()
                    };

                    let shortcut = action
                        .shortcut
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty());

                    if let Some(shortcut) = shortcut {
                        let Ok(entry) = store.record_missed(&action.action, title, shortcut) else {
                            return;
                        };
                        changed = true;
                        if entry.missed % settings.show_tips_click_count == 0 {
                            pending_tip = Some(PendingTip::Missed(entry));
                        }
                    } else if action.source != ActionSource::Other {
                        let Ok(count) = store.record_no_shortcut(&action.action) else {
                            return;
                        };
                        if settings.propose_to_create_shortcut_count > 0
                            && count % settings.propose_to_create_shortcut_count == 0
                        {
                            pending_tip = Some(PendingTip::SuggestCreate {
                                action_id: action.action.clone(),
                                title: title.to_owned(),
                                count,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(tip) = pending_tip {
            match tip {
                PendingTip::Missed(entry) => {
                    let _ = store.set_last_action(&entry.action_id);
                    if changed {
                        ui::refresh_all_ui(&store);
                    } else {
                        ui::update_status_bar(&store);
                    }
                    ui::show_missed_shortcut_tip(&entry, settings.hard_mode);
                }
                PendingTip::SuggestCreate { action_id, title, count } => {
                    let _ = store.set_last_action(&action_id);
                    if changed {
                        ui::refresh_all_ui(&store);
                    } else {
                        ui::update_status_bar(&store);
                    }
                    ui::show_create_shortcut_suggestion(&action_id, &title, count);
                }
            }
        } else if changed {
            ui::refresh_all_ui(&store);
        }
    }

    fn run_command(&mut self, id: &str, _ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
        match id {
            "open-hitlist" => {
                host::reveal_view(ui::HITLIST_VIEW)?;
                Ok(None)
            }
            "open-suppressed" => {
                host::reveal_view(ui::SUPPRESSED_VIEW)?;
                Ok(None)
            }
            "toggle-snooze" => {
                let mut store = Store::load()?;
                let next = !store.snoozed;
                store.set_snoozed(next)?;
                ui::refresh_all_ui(&store);
                let msg = if next {
                    "Key Promoter X notifications snoozed."
                } else {
                    "Key Promoter X notifications resumed."
                };
                host::notify(NotificationLevel::Info, "Key Promoter X", msg);
                Ok(Some(msg.to_owned()))
            }
            "suppress" => self.cmd_suppress(arg),
            "suppress-hitlist-row" => {
                let row: usize = arg
                    .ok_or_else(|| Error::Invalid("missing hitlist row index".into()))?
                    .parse()
                    .map_err(|_| Error::Invalid("invalid hitlist row index".into()))?;
                let mut store = Store::load()?;
                let rows = store.sorted_hitlist(self.hitlist_sort);
                let target = rows
                    .get(row)
                    .ok_or_else(|| Error::NotFound(format!("hitlist row {row}")))?
                    .clone();
                store.suppress(&target.action_id, Some(&target.title), target.shortcut.as_deref())?;
                ui::refresh_all_ui(&store);
                Ok(Some(format!("Suppressed tips for {}.", target.title)))
            }
            "unsuppress" => self.cmd_unsuppress(arg),
            "unsuppress-all" => {
                let mut store = Store::load()?;
                let count = store.unsuppress_all()?;
                ui::refresh_all_ui(&store);
                Ok(Some(format!("Restored {count} suppressed shortcut(s).")))
            }
            "suppress-last" => {
                let mut store = Store::load()?;
                let action_id = store
                    .last_action
                    .clone()
                    .ok_or_else(|| Error::NotFound("no tip has been shown yet".into()))?;
                let entry = store.suppress(&action_id, None, None)?;
                ui::refresh_all_ui(&store);
                let label = entry.map(|e| e.title).unwrap_or(action_id);
                Ok(Some(format!("Suppressed tips for {label}.")))
            }
            "statistics" => {
                let store = Store::load()?;
                let rows = store.sorted_hitlist(TableSort { column: 2, descending: true });
                let body = if rows.is_empty() {
                    "No missed shortcuts yet.".to_owned()
                } else {
                    rows.iter()
                        .take(15)
                        .map(|entry| {
                            let shortcut = entry.shortcut.as_deref().unwrap_or("—");
                            format!(
                                "{}× {} ({shortcut}) — {} keyboard hits",
                                entry.missed, entry.title, entry.hits
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                host::notify(NotificationLevel::Info, "Key Promoter X: Missed Shortcuts", &body);
                Ok(Some(body))
            }
            "reset" => {
                let mut store = Store::load()?;
                if arg != Some("force") {
                    let confirmed = host::confirm(
                        "Reset Statistics",
                        "Do you really want to reset your Key Promoter X statistics? This cannot be undone!",
                    )?;
                    if !confirmed {
                        return Ok(None);
                    }
                }
                store.reset()?;
                ui::refresh_all_ui(&store);
                Ok(Some("Key Promoter X statistics were reset.".into()))
            }
            "about" => {
                let body = "OpenIDEA port of IntelliJ Key Promoter X (BSD-3-Clause).\nOriginal by Patrick Scheibe (halirutan), Dmitry Kashin, and Athiele.";
                let open = NotificationAction::new("Open GitHub", "open-upstream");
                if host::notify_with_actions(NotificationLevel::Info, "Key Promoter X", body, &[open]).is_err() {
                    host::notify(NotificationLevel::Info, "Key Promoter X", body);
                }
                Ok(None)
            }
            "open-upstream" => {
                host::open_url(ui::UPSTREAM_URL)?;
                Ok(None)
            }
            _ => Err(Error::NotFound(format!("command {id}"))),
        }
    }
}

impl KeyPromoterX {
    fn cmd_suppress(&self, arg: Option<&str>) -> Result<Option<String>, Error> {
        let mut store = Store::load()?;
        let action_id = if let Some(id) = arg.filter(|s| !s.is_empty()) {
            id.to_owned()
        } else {
            let entries = store.sorted_hitlist(self.hitlist_sort);
            if entries.is_empty() {
                return Ok(Some("No missed shortcuts to suppress.".into()));
            }
            let items: Vec<PickItem> = entries
                .iter()
                .map(|e| {
                    let desc = format!(
                        "{} · missed {}",
                        e.shortcut.as_deref().unwrap_or("No shortcut"),
                        ui::pluralize(e.missed)
                    );
                    PickItem::new(&e.action_id, &e.title).description(&desc)
                })
                .collect();
            let Some(chosen) = host::pick("Suppress Shortcut Tips", &items)? else {
                return Ok(None);
            };
            chosen
        };

        let entry = store.suppress(&action_id, None, None)?;
        ui::refresh_all_ui(&store);
        let label = entry.map(|e| e.title).unwrap_or(action_id);
        Ok(Some(format!("Suppressed tips for {label}.")))
    }

    fn cmd_unsuppress(&self, arg: Option<&str>) -> Result<Option<String>, Error> {
        let mut store = Store::load()?;
        let action_id = if let Some(raw) = arg.filter(|s| !s.is_empty()) {
            raw.strip_prefix("suppressed:").unwrap_or(raw).to_owned()
        } else {
            let entries = store.sorted_suppressed();
            if entries.is_empty() {
                return Ok(Some("No suppressed shortcuts to restore.".into()));
            }
            let items: Vec<PickItem> = entries
                .iter()
                .map(|e| {
                    let desc = e.shortcut.as_deref().unwrap_or("No shortcut");
                    PickItem::new(&e.action_id, &e.title).description(desc)
                })
                .collect();
            let Some(chosen) = host::pick("Reactivate Shortcut Tips", &items)? else {
                return Ok(None);
            };
            chosen
        };

        let Some(entry) = store.unsuppress(&action_id)? else {
            return Err(Error::NotFound(format!("suppressed action {action_id}")));
        };
        ui::refresh_all_ui(&store);
        Ok(Some(format!("Reactivated tips for {}.", entry.title)))
    }
}

impl Views for KeyPromoterX {
    fn render(&mut self, view: &str, viewport: Option<Viewport>, _ctx: &Invocation) -> Result<View, Error> {
        let store = match Store::load() {
            Ok(s) => s,
            Err(Error::Denied(_)) => {
                return Ok(View::message(
                    "Grant the storage permission to track Key Promoter X statistics.",
                ));
            }
            Err(err) => return Err(err),
        };

        match view {
            ui::HITLIST_VIEW => Ok(ui::render_hitlist(&store, self.hitlist_sort, viewport)),
            ui::SUPPRESSED_VIEW => Ok(ui::render_suppressed(&store)),
            _ => Err(Error::NotFound(format!("view {view}"))),
        }
    }

    fn handle(&mut self, view: &str, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        if let Some(sort) = event.sort()
            && view == ui::HITLIST_VIEW
        {
            self.hitlist_sort = sort;
            host::refresh_view(ui::HITLIST_VIEW);
            return Ok(());
        }

        if let Some(row) = event.table_activate()
            && view == ui::HITLIST_VIEW
        {
            let mut store = Store::load()?;
            let rows = store.sorted_hitlist(self.hitlist_sort);
            if let Some(target) = rows.get(row as usize).cloned() {
                store.suppress(&target.action_id, Some(&target.title), target.shortcut.as_deref())?;
                ui::refresh_all_ui(&store);
            }
            return Ok(());
        }

        match event {
            ViewEvent::Button(id) => match id.as_str() {
                "toggle-snooze" => {
                    self.run_command("toggle-snooze", ctx, None)?;
                }
                "reset" => {
                    self.run_command("reset", ctx, None)?;
                }
                "unsuppress-all" => {
                    self.run_command("unsuppress-all", ctx, None)?;
                }
                "refresh" => {
                    host::refresh_view(view);
                }
                _ => {}
            },
            ViewEvent::Activate(key) if view == ui::SUPPRESSED_VIEW => {
                let action_id = key.strip_prefix("suppressed:").unwrap_or(&key);
                let mut store = Store::load()?;
                if store.unsuppress(action_id)?.is_some() {
                    ui::refresh_all_ui(&store);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

register_plugin!(KeyPromoterX, views);

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::ActionPerformed;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::views::Body;

    fn action(id: &str, title: &str, source: ActionSource, shortcut: Option<&str>) -> Event {
        Event::ActionPerformed(ActionPerformed {
            action: id.into(),
            title: title.into(),
            source,
            shortcut: shortcut.map(str::to_owned),
        })
    }

    #[test]
    fn mouse_action_records_miss_shows_action_notification_and_updates_status() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        plugin.handle_events(
            &[action("editor.duplicateLine", "Duplicate Line", ActionSource::Menu, Some("Cmd+D"))],
            &ctx,
        );

        let notes = host.notifications_with_actions();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].notification.title, "Duplicate Line (Cmd+D)");
        assert!(notes[0].notification.body.contains("missed 1 time"));
        assert_eq!(notes[0].actions.len(), 3);
        assert_eq!(notes[0].actions[0].command, "suppress");
        assert_eq!(notes[0].actions[0].argument.as_deref(), Some("editor.duplicateLine"));
        assert_eq!(notes[0].actions[1].command, "open-hitlist");
        assert_eq!(notes[0].actions[2].command, "toggle-snooze");

        let status = host.status_items();
        assert_eq!(status[ui::STATUS_ID].text, "KPX: 1");
        assert!(!host.status_menu(ui::STATUS_ID).is_empty());
    }

    #[test]
    fn show_tips_click_count_throttles_notifications_while_counting_every_miss() {
        let host = MockHost::install();
        host.set_setting("showTipsClickCount", "2");
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        let ev = action("file.save", "Save", ActionSource::Toolbar, Some("Cmd+S"));
        plugin.handle_events(&[ev.clone()], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 0);

        plugin.handle_events(&[ev.clone()], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 1);
        assert!(host.notifications_with_actions()[0].notification.body.contains("missed 2 times"));

        plugin.handle_events(&[ev], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 1);

        let store = Store::load().unwrap();
        assert_eq!(store.stats["file.save"].missed, 3);
    }

    #[test]
    fn keyboard_execution_tracks_hits_without_notifying() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        plugin.handle_events(
            &[action("edit.copy", "Copy", ActionSource::ContextMenu, Some("Cmd+C"))],
            &ctx,
        );
        plugin.handle_events(
            &[
                action("edit.copy", "Copy", ActionSource::Keyboard, Some("Cmd+C")),
                action("edit.copy", "Copy", ActionSource::Keyboard, Some("Cmd+C")),
                action("untracked.act", "Untracked", ActionSource::Keyboard, Some("Cmd+U")),
            ],
            &ctx,
        );

        assert_eq!(host.notifications_with_actions().len(), 1);
        let store = Store::load().unwrap();
        assert_eq!(store.stats["edit.copy"].missed, 1);
        assert_eq!(store.stats["edit.copy"].hits, 2);
        assert!(!store.stats.contains_key("untracked.act"));
    }

    #[test]
    fn per_source_settings_filter_events() {
        let host = MockHost::install();
        host.set_setting("menusEnabled", "false");
        host.set_setting("toolbarButtonsEnabled", "false");
        host.set_setting("contextMenuEnabled", "false");
        host.set_setting("searchEnabled", "false");
        host.set_setting("otherButtonsEnabled", "false");
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        plugin.handle_events(
            &[
                action("a", "A", ActionSource::Menu, Some("Cmd+1")),
                action("b", "B", ActionSource::Toolbar, Some("Cmd+2")),
                action("c", "C", ActionSource::ContextMenu, Some("Cmd+3")),
                action("d", "D", ActionSource::Search, Some("Cmd+4")),
                action("e", "E", ActionSource::Other, Some("Cmd+5")),
            ],
            &ctx,
        );
        assert!(host.notifications_with_actions().is_empty());
        assert!(Store::load().unwrap().stats.is_empty());

        host.set_setting("searchEnabled", "true");
        plugin.handle_events(&[action("d", "D", ActionSource::Search, Some("Cmd+4"))], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 1);
    }

    #[test]
    fn unmapped_shortcut_suggestion_triggers_on_threshold_and_respects_zero() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        let ev = action("tools.custom", "Custom Tool", ActionSource::Menu, None);
        plugin.handle_events(&[ev.clone()], &ctx);
        plugin.handle_events(&[ev.clone()], &ctx);
        assert!(host.notifications_with_actions().is_empty());

        plugin.handle_events(&[ev.clone()], &ctx);
        let notes = host.notifications_with_actions();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].notification.body.contains("Want to create a shortcut for \"Custom Tool\"?"));

        host.set_setting("proposeToCreateShortcutCount", "0");
        plugin.handle_events(&[ev.clone(), ev.clone(), ev], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 1);
    }

    #[test]
    fn suppress_and_unsuppress_preserve_counts_and_stop_tips() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        let ev = action("nav.back", "Back", ActionSource::Toolbar, Some("Cmd+["));
        plugin.handle_events(&[ev.clone(), ev.clone()], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 1);

        plugin.run_command("suppress-last", &ctx, None).unwrap();
        plugin.handle_events(&[ev.clone()], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 1, "suppressed action should not notify");

        let store = Store::load().unwrap();
        assert!(!store.stats.contains_key("nav.back"));
        assert_eq!(store.suppressed["nav.back"].missed, 2);

        plugin.run_command("unsuppress", &ctx, Some("suppressed:nav.back")).unwrap();
        let store = Store::load().unwrap();
        assert!(store.suppressed.is_empty());
        assert_eq!(store.stats["nav.back"].missed, 2);

        plugin.handle_events(&[ev], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 2);
    }

    #[test]
    fn snooze_pauses_tracking_and_updates_status() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        plugin.run_command("toggle-snooze", &ctx, None).unwrap();
        assert_eq!(host.status_items()[ui::STATUS_ID].text, "KPX: Snoozed");

        let ev = action("run.build", "Build Project", ActionSource::Menu, Some("Cmd+F9"));
        plugin.handle_events(&[ev.clone()], &ctx);
        assert!(host.notifications_with_actions().is_empty());
        assert!(Store::load().unwrap().stats.is_empty());

        plugin.run_command("toggle-snooze", &ctx, None).unwrap();
        assert_eq!(host.status_items()[ui::STATUS_ID].text, "KPX");

        plugin.handle_events(&[ev], &ctx);
        assert_eq!(host.notifications_with_actions().len(), 1);
    }

    #[test]
    fn self_actions_are_ignored() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        plugin.handle_events(
            &[action(
                "plugin.xirf.key-promoter-x.open-hitlist",
                "Key Promoter X: Show Missed Shortcuts",
                ActionSource::Menu,
                Some("Alt+K"),
            )],
            &ctx,
        );
        assert!(host.notifications_with_actions().is_empty());
        assert!(Store::load().unwrap().stats.is_empty());
    }

    #[test]
    fn views_render_sort_and_activate_rows() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        plugin.handle_events(&[action("a.one", "Alpha", ActionSource::Menu, Some("Cmd+1"))], &ctx);
        plugin.handle_events(
            &[
                action("b.two", "Beta", ActionSource::Menu, Some("Cmd+2")),
                action("b.two", "Beta", ActionSource::Menu, Some("Cmd+2")),
            ],
            &ctx,
        );

        let view = plugin.render(ui::HITLIST_VIEW, None, &ctx).unwrap();
        let Body::Other { kind, data } = view.body else {
            panic!("expected sortable table");
        };
        assert_eq!(kind, "table");
        assert!(data.contains(r#"["Beta","Cmd+2","2","0"],["Alpha","Cmd+1","1","0"]"#));

        // Sort ascending by Title (column 0).
        plugin
            .handle(
                ui::HITLIST_VIEW,
                ViewEvent::Other {
                    kind: "sort".into(),
                    data: r#"{"column":0,"descending":false}"#.into(),
                },
                &ctx,
            )
            .unwrap();
        let view = plugin.render(ui::HITLIST_VIEW, None, &ctx).unwrap();
        let Body::Other { data, .. } = view.body else {
            panic!("expected sortable table");
        };
        assert!(data.contains(r#"["Alpha","Cmd+1","1","0"],["Beta","Cmd+2","2","0"]"#));

        // Double-click row 0 ("Alpha") to suppress it.
        plugin
            .handle(
                ui::HITLIST_VIEW,
                ViewEvent::Other {
                    kind: "table-activate".into(),
                    data: r#"{"row":0}"#.into(),
                },
                &ctx,
            )
            .unwrap();

        let suppressed_view = plugin.render(ui::SUPPRESSED_VIEW, None, &ctx).unwrap();
        let Body::List(items) = suppressed_view.body else {
            panic!("expected suppressed list");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "suppressed:a.one");

        // Double-click suppressed item to reactivate it.
        plugin
            .handle(ui::SUPPRESSED_VIEW, ViewEvent::Activate("suppressed:a.one".into()), &ctx)
            .unwrap();
        assert!(Store::load().unwrap().suppressed.is_empty());
        assert!(Store::load().unwrap().stats.contains_key("a.one"));
    }

    #[test]
    fn hard_mode_uses_confirm_dialog() {
        let host = MockHost::install();
        host.set_setting("hardMode", "true");
        host.answer_next_prompt(Some(&["ok"]));
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        plugin.handle_events(
            &[action("edit.find", "Find", ActionSource::Menu, Some("Cmd+F"))],
            &ctx,
        );

        assert!(host.notifications_with_actions().is_empty(), "hard mode should confirm instead of notify");
        assert_eq!(Store::load().unwrap().stats["edit.find"].missed, 1);
    }

    #[test]
    fn reset_respects_confirmation_dialog() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        plugin.handle_events(&[action("edit.find", "Find", ActionSource::Menu, Some("Cmd+F"))], &ctx);

        // Cancel reset.
        host.answer_next_prompt(None);
        plugin.run_command("reset", &ctx, None).unwrap();
        assert_eq!(Store::load().unwrap().stats.len(), 1);

        // Confirm reset.
        host.answer_next_prompt(Some(&["ok"]));
        plugin.run_command("reset", &ctx, None).unwrap();
        assert!(Store::load().unwrap().stats.is_empty());
        assert_eq!(host.status_items()[ui::STATUS_ID].text, "KPX");
    }

    #[test]
    fn without_storage_degrades_gracefully() {
        let host = MockHost::install().with_capabilities(&["events.input"]);
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        let activate_ctx = Context {
            plugin_id: "xirf.key-promoter-x".into(),
            plugin_version: "0.1.0".into(),
            api_version: "1.14".into(),
            capabilities: vec!["events.input".into()],
        };
        plugin.activate(&activate_ctx).unwrap();
        plugin.handle_events(&[action("a", "A", ActionSource::Menu, Some("Cmd+1"))], &ctx);
        assert!(host.notifications_with_actions().is_empty());

        let view = plugin.render(ui::HITLIST_VIEW, None, &ctx).unwrap();
        assert!(matches!(view.body, Body::Message(_)));

        assert_eq!(
            plugin.run_command("statistics", &ctx, None).unwrap_err(),
            Error::Denied("storage".into())
        );
    }

    #[test]
    fn automatic_other_actions_are_ignored_with_or_without_shortcut() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        for _ in 0..6 {
            plugin.handle_events(
                &[
                    action("project.refresh", "Reload Project Tree from Disk", ActionSource::Other, None),
                    action("vcs.refresh", "Refresh VCS Status", ActionSource::Other, None),
                    action("editor.showHint", "Show Hint", ActionSource::Other, None),
                ],
                &ctx,
            );
        }
        plugin.handle_events(
            &[
                action(
                    "project.refresh",
                    "Reload Project Tree from Disk",
                    ActionSource::Other,
                    Some("Cmd+Alt+Y"),
                ),
                action("vcs.refresh", "Refresh VCS Status", ActionSource::Other, Some("Cmd+Alt+Y")),
            ],
            &ctx,
        );

        assert!(host.notifications_with_actions().is_empty());
        let store = Store::load().unwrap();
        assert!(store.stats.is_empty());
        assert!(store.no_shortcut_counts.is_empty());

        let menu_refresh = action("project.refresh", "Reload Project Tree from Disk", ActionSource::Menu, None);
        plugin.handle_events(&[menu_refresh.clone(), menu_refresh.clone(), menu_refresh], &ctx);

        let notes = host.notifications_with_actions();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].notification.body.contains("Reload Project Tree from Disk"));
    }

    #[test]
    fn unmapped_other_source_actions_do_not_suggest_creating_shortcut() {
        let host = MockHost::install();
        let ctx = host.invocation();
        let mut plugin = KeyPromoterX::default();

        for _ in 0..6 {
            plugin.handle_events(
                &[action("window.selectTab", "Select Tab", ActionSource::Other, None)],
                &ctx,
            );
        }
        assert!(host.notifications_with_actions().is_empty());
        assert!(Store::load().unwrap().no_shortcut_counts.is_empty());

        plugin.handle_events(
            &[action("run.reload", "Reload", ActionSource::Other, Some("Cmd+R"))],
            &ctx,
        );
        let notes = host.notifications_with_actions();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].notification.title, "Reload (Cmd+R)");
        assert_eq!(Store::load().unwrap().stats["run.reload"].missed, 1);
    }
}
