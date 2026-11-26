// InventoryManager filter-rule editor: a structured view over the
// `profiles` setting (the reference's `InventoryFilterRuleEditorFrame`).
//
// The reference's editor also hosts the shared-preset condition builder;
// this port edits the item rules (filter, action, removal) on top of the
// same `Profile`/`Rule` JSON the runtime already parses.
use super::View;
use crate::inventory::rules::{parse_presets, parse_profiles, Action, Filter, Profile, Rule};
use serde_json::json;

pub const PROFILES: &str = "inventory.inventory-manager.profiles";
pub const PRESETS: &str = "inventory.inventory-manager.presets";

#[derive(Clone, Debug)]
pub struct RulesEditor {
    /// The profile being edited; falls back to the first one.
    pub profile: String,
    /// The new rule's filter text: a matcher id/name or an item id.
    pub item: String,
    /// The new rule's action.
    pub action: Action,
}

impl RulesEditor {
    pub fn new(profiles: &[Profile]) -> Self {
        Self {
            profile: profiles.first().map(|p| p.name.clone()).unwrap_or_default(),
            item: String::new(),
            action: Action::Remove,
        }
    }

    pub fn selected<'a>(&self, profiles: &'a [Profile]) -> Option<&'a Profile> {
        profiles
            .iter()
            .find(|p| p.name == self.profile)
            .or_else(|| profiles.first())
    }

    fn selected_mut<'a>(&self, profiles: &'a mut [Profile]) -> Option<&'a mut Profile> {
        let name = self.profile.clone();
        if profiles.iter().any(|p| p.name == name) {
            profiles.iter_mut().find(|p| p.name == name)
        } else {
            profiles.first_mut()
        }
    }

    pub fn cycle_action(&mut self) {
        let i = Action::ALL
            .iter()
            .position(|a| *a == self.action)
            .unwrap_or(0);
        self.action = Action::ALL[(i + 1) % Action::ALL.len()];
    }
}

/// Parses the `profiles` setting with the `presets` setting's shared rules.
pub fn profiles(view: &View) -> Vec<Profile> {
    let presets =
        parse_presets(view.values[PRESETS].as_str().unwrap_or_default()).unwrap_or_default();
    parse_profiles(
        view.values[PROFILES].as_str().unwrap_or_default(),
        &presets,
    )
    .unwrap_or_default()
}

/// Serializes the profiles back into the setting's JSON array.
pub fn text(profiles: &[Profile]) -> String {
    serde_json::Value::Array(profiles.iter().map(Profile::to_json).collect()).to_string()
}

/// Appends the editor's filter as an item rule on the selected profile.
pub fn add(profiles: &mut [Profile], editor: &RulesEditor) -> Result<(), String> {
    let item = editor.item.trim();
    if item.is_empty() {
        return Err("Enter an item or matcher id".into());
    }
    let profile = editor
        .selected_mut(profiles)
        .ok_or("No inventory profile")?;
    profile.items.push(Rule {
        filter: Filter::parse(Some(&json!(item))),
        preset: None,
        shared: None,
        priority: None,
        slot: None,
        action: editor.action,
    });
    Ok(())
}

pub fn remove(profiles: &mut [Profile], editor: &RulesEditor, index: usize) -> bool {
    let Some(profile) = editor.selected_mut(profiles) else {
        return false;
    };
    if index < profile.items.len() {
        profile.items.remove(index);
        true
    } else {
        false
    }
}

pub fn cycle(profiles: &mut [Profile], editor: &RulesEditor, index: usize) -> bool {
    let Some(rule) = editor
        .selected_mut(profiles)
        .and_then(|p| p.items.get_mut(index))
    else {
        return false;
    };
    let i = Action::ALL
        .iter()
        .position(|a| *a == rule.action)
        .unwrap_or(0);
    rule.action = Action::ALL[(i + 1) % Action::ALL.len()];
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn profiles() -> Vec<Profile> {
        parse_profiles(
            r#"[{"name":"Inventory #1","slots":[],"inventoryFilters":[{"itemFilter":"blocks","filterAction":"Drop item"}],"armor_mode":"No armor management"}]"#,
            &[],
        )
        .unwrap()
    }

    #[test]
    fn the_editor_adds_a_matcher_and_an_item_rule() {
        let mut list = profiles();
        let mut editor = RulesEditor::new(&list);
        editor.item = "swords".into();
        add(&mut list, &editor).unwrap();
        editor.item = "minecraft:diamond_sword".into();
        editor.action = Action::Move;
        add(&mut list, &editor).unwrap();
        let items = &list[0].items;
        assert_eq!(items[1].filter, Filter::Matcher(crate::inventory::rules::Matcher::Swords));
        assert_eq!(items[2].filter, Filter::Item("diamond_sword".into()));
        assert_eq!(items[2].action, Action::Move);
    }

    #[test]
    fn removing_and_cycling_keep_the_rest_in_order() {
        let mut list = profiles();
        let mut editor = RulesEditor::new(&list);
        editor.item = "swords".into();
        add(&mut list, &editor).unwrap();
        assert!(cycle(&mut list, &editor, 0));
        assert_eq!(list[0].items[0].action, Action::Move);
        assert!(remove(&mut list, &editor, 1));
        assert_eq!(list[0].items.len(), 1);
        assert!(!remove(&mut list, &editor, 3));
        assert!(!cycle(&mut list, &editor, 3));
    }

    #[test]
    fn the_setting_text_round_trips_the_edited_profile() {
        let mut list = profiles();
        let mut editor = RulesEditor::new(&list);
        editor.item = "any-food".into();
        add(&mut list, &editor).unwrap();
        let text = text(&list);
        let parsed = parse_profiles(&text, &[]).unwrap();
        assert_eq!(parsed, list);
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0]["inventoryFilters"][1]["itemFilter"], json!("any-food"));
    }
}
