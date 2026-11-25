use std::cmp::Ordering;
use std::collections::BTreeMap;

use openidea_plugin::views::TableSort;
use openidea_plugin::{Error, host::storage};
use serde::{Deserialize, Serialize};

const STATS_KEY: &str = "kpx:stats";
const SUPPRESSED_KEY: &str = "kpx:suppressed";
const NO_SHORTCUT_KEY: &str = "kpx:no-shortcut";
const LAST_ACTION_KEY: &str = "kpx:last-action";
const SNOOZED_KEY: &str = "kpx:snoozed";

/// Statistics recorded for a single IDE action.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatEntry {
    pub action_id: String,
    pub title: String,
    pub shortcut: Option<String>,
    pub missed: u32,
    pub hits: u32,
}

/// Persistent plugin state backed by `host::storage`.
#[derive(Clone, Debug, Default)]
pub struct Store {
    pub stats: BTreeMap<String, StatEntry>,
    pub suppressed: BTreeMap<String, StatEntry>,
    pub no_shortcut_counts: BTreeMap<String, u32>,
    pub last_action: Option<String>,
    pub snoozed: bool,
}

impl Store {
    /// Loads persisted state from `host::storage`. Propagates `Error::Denied`
    /// if the storage capability is unavailable; corrupt JSON falls back to empty maps.
    pub fn load() -> Result<Self, Error> {
        let stats = load_json_map(STATS_KEY)?;
        let suppressed = load_json_map(SUPPRESSED_KEY)?;
        let no_shortcut_counts = load_json_map(NO_SHORTCUT_KEY)?;
        let last_action = storage::get(LAST_ACTION_KEY)?.filter(|s| !s.is_empty());
        let snoozed = storage::get(SNOOZED_KEY)?.as_deref() == Some("1");
        Ok(Self { stats, suppressed, no_shortcut_counts, last_action, snoozed })
    }

    pub fn is_suppressed(&self, action_id: &str) -> bool {
        self.suppressed.contains_key(action_id)
    }

    /// Records a mouse invocation of an action that has a keyboard shortcut.
    pub fn record_missed(&mut self, action_id: &str, title: &str, shortcut: &str) -> Result<StatEntry, Error> {
        let entry = self.stats.entry(action_id.to_owned()).or_insert_with(|| StatEntry {
            action_id: action_id.to_owned(),
            title: title.to_owned(),
            shortcut: Some(shortcut.to_owned()),
            missed: 0,
            hits: 0,
        });
        entry.title = title.to_owned();
        entry.shortcut = Some(shortcut.to_owned());
        entry.missed = entry.missed.saturating_add(1);
        let result = entry.clone();
        self.save_stats()?;
        Ok(result)
    }

    /// Increments keyboard usage hits for an action already tracked in the hitlist.
    pub fn record_keyboard_hit(&mut self, action_id: &str) -> Result<bool, Error> {
        if let Some(entry) = self.stats.get_mut(action_id) {
            entry.hits = entry.hits.saturating_add(1);
            self.save_stats()?;
            return Ok(true);
        }
        if let Some(entry) = self.suppressed.get_mut(action_id) {
            entry.hits = entry.hits.saturating_add(1);
            self.save_suppressed()?;
        }
        Ok(false)
    }

    /// Increments click count for an action without an assigned keyboard shortcut.
    pub fn record_no_shortcut(&mut self, action_id: &str) -> Result<u32, Error> {
        let count = self.no_shortcut_counts.entry(action_id.to_owned()).or_insert(0);
        *count = count.saturating_add(1);
        let new_count = *count;
        self.save_no_shortcut()?;
        Ok(new_count)
    }

    /// Records the last action that displayed a tip or suggestion.
    pub fn set_last_action(&mut self, action_id: &str) -> Result<(), Error> {
        self.last_action = Some(action_id.to_owned());
        storage::set(LAST_ACTION_KEY, action_id)
    }

    /// Moves an action into the suppressed list, preserving any accumulated stats.
    pub fn suppress(
        &mut self,
        action_id: &str,
        fallback_title: Option<&str>,
        fallback_shortcut: Option<&str>,
    ) -> Result<Option<StatEntry>, Error> {
        let had_no_shortcut = self.no_shortcut_counts.remove(action_id).is_some();
        let removed = self.stats.remove(action_id);
        let entry = if let Some(existing) = removed {
            existing
        } else if let Some(already) = self.suppressed.get(action_id) {
            return Ok(Some(already.clone()));
        } else {
            StatEntry {
                action_id: action_id.to_owned(),
                title: fallback_title.unwrap_or(action_id).to_owned(),
                shortcut: fallback_shortcut.map(str::to_owned),
                missed: 0,
                hits: 0,
            }
        };
        let saved = entry.clone();
        self.suppressed.insert(action_id.to_owned(), entry);
        self.save_stats()?;
        self.save_suppressed()?;
        if had_no_shortcut {
            self.save_no_shortcut()?;
        }
        Ok(Some(saved))
    }

    /// Removes an action from the suppressed list and restores it to the hitlist if it had misses.
    pub fn unsuppress(&mut self, action_id: &str) -> Result<Option<StatEntry>, Error> {
        let Some(entry) = self.suppressed.remove(action_id) else {
            return Ok(None);
        };
        if entry.missed > 0 {
            self.stats.insert(action_id.to_owned(), entry.clone());
            self.save_stats()?;
        }
        self.save_suppressed()?;
        Ok(Some(entry))
    }

    /// Restores all suppressed entries (returning any with `missed > 0` to the hitlist).
    pub fn unsuppress_all(&mut self) -> Result<usize, Error> {
        if self.suppressed.is_empty() {
            return Ok(0);
        }
        let entries = std::mem::take(&mut self.suppressed);
        let count = entries.len();
        let mut restored_any = false;
        for (id, entry) in entries {
            if entry.missed > 0 {
                self.stats.insert(id, entry);
                restored_any = true;
            }
        }
        if restored_any {
            self.save_stats()?;
        }
        self.save_suppressed()?;
        Ok(count)
    }

    /// Clears missed statistics, suppressed entries, and unmapped shortcut counters.
    pub fn reset(&mut self) -> Result<(), Error> {
        self.stats.clear();
        self.suppressed.clear();
        self.no_shortcut_counts.clear();
        self.last_action = None;
        storage::delete(STATS_KEY)?;
        storage::delete(SUPPRESSED_KEY)?;
        storage::delete(NO_SHORTCUT_KEY)?;
        storage::delete(LAST_ACTION_KEY)?;
        Ok(())
    }

    pub fn set_snoozed(&mut self, snoozed: bool) -> Result<(), Error> {
        self.snoozed = snoozed;
        if snoozed {
            storage::set(SNOOZED_KEY, "1")
        } else {
            storage::delete(SNOOZED_KEY)
        }
    }

    /// Returns hitlist rows sorted according to `sort`.
    pub fn sorted_hitlist(&self, sort: TableSort) -> Vec<StatEntry> {
        let mut items: Vec<StatEntry> = self.stats.values().cloned().collect();
        items.sort_by(|a, b| {
            let ord = match sort.column {
                0 => cmp_ci(&a.title, &b.title),
                1 => cmp_ci(a.shortcut.as_deref().unwrap_or(""), b.shortcut.as_deref().unwrap_or("")),
                3 => a.hits.cmp(&b.hits),
                _ => a.missed.cmp(&b.missed),
            };
            let ord = if sort.descending { ord.reverse() } else { ord };
            ord.then_with(|| cmp_ci(&a.title, &b.title))
                .then_with(|| a.action_id.cmp(&b.action_id))
        });
        items
    }

    /// Returns suppressed entries sorted alphabetically by title.
    pub fn sorted_suppressed(&self) -> Vec<StatEntry> {
        let mut items: Vec<StatEntry> = self.suppressed.values().cloned().collect();
        items.sort_by(|a, b| cmp_ci(&a.title, &b.title).then_with(|| a.action_id.cmp(&b.action_id)));
        items
    }

    /// Returns `(distinct_actions, total_missed, total_hits)`.
    pub fn totals(&self) -> (usize, u32, u32) {
        let mut missed = 0u32;
        let mut hits = 0u32;
        for entry in self.stats.values() {
            missed = missed.saturating_add(entry.missed);
            hits = hits.saturating_add(entry.hits);
        }
        (self.stats.len(), missed, hits)
    }

    fn save_stats(&self) -> Result<(), Error> {
        save_json(STATS_KEY, &self.stats)
    }

    fn save_suppressed(&self) -> Result<(), Error> {
        save_json(SUPPRESSED_KEY, &self.suppressed)
    }

    fn save_no_shortcut(&self) -> Result<(), Error> {
        save_json(NO_SHORTCUT_KEY, &self.no_shortcut_counts)
    }
}

fn load_json_map<T: for<'de> Deserialize<'de>>(key: &str) -> Result<BTreeMap<String, T>, Error> {
    Ok(storage::get(key)?
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default())
}

fn save_json<T: Serialize>(key: &str, value: &T) -> Result<(), Error> {
    let serialized = serde_json::to_string(value).map_err(|e| Error::Failed(e.to_string()))?;
    storage::set(key, &serialized)
}

fn cmp_ci(a: &str, b: &str) -> Ordering {
    a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()).then_with(|| a.cmp(b))
}
