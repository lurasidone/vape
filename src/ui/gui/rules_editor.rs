// InventoryManager filter-rule editor: a structured view over the
// `profiles` and `presets` settings (the reference's
// `InventoryFilterRuleEditorFrame` and its custom-rule condition builder).
//
// The reference's editor also has the full condition UI (operators, several
// texts, multiple AND/OR groups); this port edits one condition group per
// preset with a kind, one value and the common comparison, plus the item and
// hotbar-slot rules.
use super::View;
use crate::inventory::rules::{
    parse_presets, parse_profiles, Action, Compare, Condition, ConditionGroup, EffectMode, Filter,
    Matcher, Preset, Profile, Rule, TextMode,
};
use serde_json::json;

pub const PROFILES: &str = "inventory.inventory-manager.profiles";
pub const PRESETS: &str = "inventory.inventory-manager.presets";

/// The editor's tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    Items,
    Slots,
    Presets,
}

/// The condition kinds the preset editor can add.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionKind {
    Durability,
    AttackDamage,
    Enchantment,
    Potion,
    Quantity,
    DisplayName,
    ItemName,
    Material,
    Lore,
}

impl ConditionKind {
    pub const ALL: [Self; 9] = [
        Self::Durability,
        Self::AttackDamage,
        Self::Enchantment,
        Self::Potion,
        Self::Quantity,
        Self::DisplayName,
        Self::ItemName,
        Self::Material,
        Self::Lore,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Durability => "Durability",
            Self::AttackDamage => "Attack damage",
            Self::Enchantment => "Enchantment",
            Self::Potion => "Potion effect",
            Self::Quantity => "Quantity",
            Self::DisplayName => "Display name",
            Self::ItemName => "Item name",
            Self::Material => "Material",
            Self::Lore => "Lore",
        }
    }

    /// Builds the condition from the editor's single value text.
    pub fn build(self, text: &str) -> Result<Condition, String> {
        let value = text.trim();
        if value.is_empty() {
            return Err("Enter the condition value".into());
        }
        let number = |v: &str| {
            v.parse::<f64>()
                .map_err(|_| "Enter a number for this condition".to_string())
        };
        Ok(match self {
            Self::Durability => Condition::Durability {
                percentage: true,
                operator: Compare::GreaterEq,
                value: number(value)? as i32,
            },
            Self::AttackDamage => Condition::AttackDamage {
                operator: Compare::GreaterEq,
                value: number(value)?,
            },
            Self::Enchantment => Condition::Enchantment {
                level_mode: false,
                enchantment: unprefixed(value),
                operator: Compare::GreaterEq,
                level: 1,
            },
            Self::Potion => Condition::PotionEffect {
                mode: EffectMode::Has,
                effect: unprefixed(value),
                operator: Compare::GreaterEq,
                value: 0,
            },
            Self::Quantity => Condition::Quantity {
                operator: Compare::GreaterEq,
                amount: number(value)? as i32,
            },
            Self::DisplayName => Condition::DisplayName {
                mode: TextMode::Contains,
                texts: texts(value),
            },
            Self::ItemName => Condition::ItemName {
                mode: TextMode::Contains,
                texts: texts(value),
            },
            Self::Lore => Condition::Lore {
                mode: TextMode::Contains,
                texts: texts(value),
            },
            Self::Material => Condition::Material {
                include: true,
                items: value
                    .split(',')
                    .map(unprefixed)
                    .filter(|s| !s.is_empty())
                    .collect(),
            },
        })
    }
}

fn unprefixed(id: &str) -> String {
    let id = id.trim().to_ascii_lowercase();
    id.strip_prefix("minecraft:").unwrap_or(&id).to_string()
}

/// Splits the editor's comma-separated text into the condition's entries.
fn texts(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A one-line description of a saved condition for the preset list.
pub fn summary(condition: &Condition) -> String {
    match condition {
        Condition::Durability {
            percentage,
            operator,
            value,
        } => format!(
            "Durability {} {value}{}",
            operator.name(),
            if *percentage { "%" } else { "" }
        ),
        Condition::AttackDamage { operator, value } => {
            format!("Attack damage {} {value}", operator.name())
        }
        Condition::Enchantment {
            level_mode,
            enchantment,
            operator,
            level,
        } => {
            if *level_mode {
                format!("{enchantment} {} {level}", operator.name())
            } else {
                format!("Has {enchantment}")
            }
        }
        Condition::PotionEffect {
            mode,
            effect,
            operator,
            value,
        } => match mode {
            EffectMode::Has => format!("Has {effect}"),
            EffectMode::Level => format!("{effect} level {} {value}", operator.name()),
            EffectMode::Duration => format!("{effect} duration {} {value}", operator.name()),
        },
        Condition::Quantity { operator, amount } => {
            format!("Quantity {} {amount}", operator.name())
        }
        Condition::DisplayName { mode, texts } => {
            format!("Display name {} {}", mode.name(), texts.join(", "))
        }
        Condition::ItemName { mode, texts } => {
            format!("Item name {} {}", mode.name(), texts.join(", "))
        }
        Condition::Lore { mode, texts } => {
            format!("Lore {} {}", mode.name(), texts.join(", "))
        }
        Condition::Material { include, items } => format!(
            "Material {} {}",
            if *include { "is in" } else { "is not in" },
            items.join(", ")
        ),
    }
}

/// Cycles a condition's comparison: numeric operators, text modes or the
/// material include flag.
pub fn cycle_operator(presets: &mut [Preset], editor: &RulesEditor, index: usize) -> bool {
    if !editor.has_group(presets) {
        return false;
    }
    let Some(condition) = editor
        .selected_group_mut(presets)
        .and_then(|g| g.conditions.get_mut(index))
    else {
        return false;
    };
    match condition {
        Condition::Durability { operator, .. }
        | Condition::AttackDamage { operator, .. }
        | Condition::Enchantment { operator, .. }
        | Condition::PotionEffect { operator, .. }
        | Condition::Quantity { operator, .. } => {
            let i = Compare::ALL
                .iter()
                .position(|c| c == operator)
                .unwrap_or(0);
            *operator = Compare::ALL[(i + 1) % Compare::ALL.len()];
        }
        Condition::DisplayName { mode, .. }
        | Condition::ItemName { mode, .. }
        | Condition::Lore { mode, .. } => {
            let i = TextMode::ALL
                .iter()
                .position(|m| m == mode)
                .unwrap_or(0);
            *mode = TextMode::ALL[(i + 1) % TextMode::ALL.len()];
        }
        Condition::Material { include, .. } => *include = !*include,
    }
    true
}

#[derive(Clone, Debug)]
pub struct RulesEditor {
    /// The profile being edited; falls back to the first one.
    pub profile: String,
    /// The new rule's filter text: a matcher id/name or an item id.
    pub item: String,
    /// The new rule's action.
    pub action: Action,
    pub tab: Tab,
    /// The selected hotbar slot (0..9) in the slot tab.
    pub slot: Option<u8>,
    /// The selected preset's uuid: the preset tab's target and the new item
    /// rule's custom rule.
    pub preset: Option<String>,
    /// The selected AND group of the preset (OR groups across the preset).
    pub group: usize,
    /// The kind of the next preset condition.
    pub kind: ConditionKind,
    /// The matcher picker is open over the editor.
    pub picker: bool,
    /// The text condition being edited (its entries load into the field and
    /// ADD replaces them instead of adding a condition).
    pub editing: Option<usize>,
}

impl RulesEditor {
    pub fn new(profiles: &[Profile]) -> Self {
        Self {
            profile: profiles.first().map(|p| p.name.clone()).unwrap_or_default(),
            item: String::new(),
            action: Action::Remove,
            tab: Tab::Items,
            slot: None,
            preset: None,
            group: 0,
            kind: ConditionKind::Durability,
            picker: false,
            editing: None,
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

    pub fn selected_preset<'a>(&self, presets: &'a [Preset]) -> Option<&'a Preset> {
        self.preset
            .as_ref()
            .and_then(|uuid| presets.iter().find(|p| &p.uuid == uuid))
            .or_else(|| presets.first())
    }

    fn selected_preset_mut<'a>(&self, presets: &'a mut [Preset]) -> Option<&'a mut Preset> {
        if let Some(index) = self
            .preset
            .as_ref()
            .and_then(|uuid| presets.iter().position(|p| &p.uuid == uuid))
        {
            return Some(&mut presets[index]);
        }
        presets.first_mut()
    }

    /// The selected group, creating empty groups up to it (the editor's NEW
    /// group path is the only other creator).
    fn selected_group_mut<'a>(&self, presets: &'a mut [Preset]) -> Option<&'a mut ConditionGroup> {
        let group = self.group;
        let preset = self.selected_preset_mut(presets)?;
        while preset.groups.len() <= group {
            preset.groups.push(ConditionGroup::default());
        }
        preset.groups.get_mut(group)
    }

    fn has_group(&self, presets: &[Preset]) -> bool {
        self.selected_preset(presets)
            .is_some_and(|p| self.group < p.groups.len())
    }

    pub fn preset_label(&self, presets: &[Preset]) -> String {
        self.selected_preset(presets)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "None".into())
    }

    pub fn cycle_action(&mut self) {
        let i = Action::ALL
            .iter()
            .position(|a| *a == self.action)
            .unwrap_or(0);
        self.action = Action::ALL[(i + 1) % Action::ALL.len()];
    }

    pub fn cycle_kind(&mut self) {
        let i = ConditionKind::ALL
            .iter()
            .position(|k| *k == self.kind)
            .unwrap_or(0);
        self.kind = ConditionKind::ALL[(i + 1) % ConditionKind::ALL.len()];
    }

    /// Cycles the item rule's custom rule: None, then each preset.
    pub fn cycle_preset(&mut self, presets: &[Preset]) {
        let current = self
            .preset
            .as_ref()
            .and_then(|uuid| presets.iter().position(|p| &p.uuid == uuid));
        self.preset = match current {
            None if presets.is_empty() => None,
            None => Some(presets[0].uuid.clone()),
            Some(i) if i + 1 < presets.len() => Some(presets[i + 1].uuid.clone()),
            Some(_) => None,
        };
    }
}

/// The reference's default profile, created when the setting is empty.
pub fn default_profile() -> Profile {
    Profile {
        name: "Inventory #1".into(),
        ..Profile::default()
    }
}

/// Parses the `profiles` setting with the `presets` setting's shared rules,
/// reporting invalid text instead of treating it as empty.
pub fn parse(view: &View) -> Result<Vec<Profile>, String> {
    let text = view.values[PROFILES].as_str().unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    parse_profiles(text, &parse_presets_setting(view)?)
}

/// Parses the `presets` setting, reporting invalid text.
pub fn parse_presets_setting(view: &View) -> Result<Vec<Preset>, String> {
    let text = view.values[PRESETS].as_str().unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    parse_presets(text)
}

/// The display view of the profiles; invalid text shows as none and the
/// mutation paths use `parse` so they never overwrite it.
pub fn profiles(view: &View) -> Vec<Profile> {
    parse(view).unwrap_or_default()
}

/// The display view of the presets.
pub fn presets(view: &View) -> Vec<Preset> {
    parse_presets_setting(view).unwrap_or_default()
}

/// Serializes the profiles back into the setting's JSON array.
pub fn text(profiles: &[Profile]) -> String {
    serde_json::Value::Array(profiles.iter().map(Profile::to_json).collect()).to_string()
}

/// Serializes the presets back into the setting's JSON array.
pub fn presets_text(presets: &[Preset]) -> String {
    serde_json::Value::Array(presets.iter().map(Preset::to_json).collect()).to_string()
}

/// Appends the editor's filter as an item rule on the selected profile,
/// attaching the selected preset as its custom rule.
pub fn add(profiles: &mut [Profile], presets: &[Preset], editor: &RulesEditor) -> Result<(), String> {
    let item = editor.item.trim();
    if item.is_empty() {
        return Err("Enter an item or matcher id".into());
    }
    let (preset, shared) = editor
        .selected_preset(presets)
        .filter(|_| editor.preset.is_some())
        .map(|p| (Some(p.clone()), Some(p.uuid.clone())))
        .unwrap_or((None, None));
    let profile = editor
        .selected_mut(profiles)
        .ok_or("No inventory profile")?;
    profile.items.push(Rule {
        filter: Filter::parse(Some(&json!(item))),
        preset,
        shared,
        priority: None,
        slot: None,
        action: editor.action,
    });
    Ok(())
}

/// Sets the selected hotbar slot's filter from the editor's filter text.
pub fn set_slot(profiles: &mut [Profile], editor: &RulesEditor) -> Result<(), String> {
    let item = editor.item.trim();
    if item.is_empty() {
        return Err("Enter an item or matcher id".into());
    }
    let Some(slot) = editor.slot.filter(|s| *s < 9) else {
        return Err("Select a hotbar slot".into());
    };
    let profile = editor
        .selected_mut(profiles)
        .ok_or("No inventory profile")?;
    profile.slot_rule_mut(slot).filter = Filter::parse(Some(&json!(item)));
    Ok(())
}

/// Adds a preset with a unique uuid and selects it.
pub fn add_preset(presets: &mut Vec<Preset>, editor: &mut RulesEditor) {
    let mut n = presets.len() + 1;
    let uuid = loop {
        let candidate = format!("preset-{n}");
        if !presets.iter().any(|p| p.uuid == candidate) {
            break candidate;
        }
        n += 1;
    };
    presets.push(Preset {
        uuid: uuid.clone(),
        name: format!("Rule #{n}"),
        groups: Vec::new(),
    });
    editor.preset = Some(uuid);
    editor.group = 0;
}

/// Adds the editor's kind/value as a condition in the selected preset's first
/// group (created on demand).
pub fn add_condition(presets: &mut [Preset], editor: &RulesEditor) -> Result<(), String> {
    let condition = editor.kind.build(&editor.item)?;
    let group = editor
        .selected_group_mut(presets)
        .ok_or("Create a preset first")?;
    group.conditions.push(condition);
    Ok(())
}

/// Adds an empty AND group to the selected preset and selects it.
pub fn add_group(presets: &mut [Preset], editor: &mut RulesEditor) -> bool {
    let len = match editor.selected_preset_mut(presets) {
        Some(preset) => {
            preset.groups.push(ConditionGroup::default());
            preset.groups.len()
        }
        None => return false,
    };
    editor.group = len - 1;
    true
}

/// The selected text condition's entries, joined for the editor field.
pub fn text_entries(presets: &[Preset], editor: &RulesEditor, index: usize) -> Option<String> {
    let condition = editor
        .selected_preset(presets)?
        .groups
        .get(editor.group)?
        .conditions
        .get(index)?;
    match condition {
        Condition::DisplayName { texts, .. }
        | Condition::ItemName { texts, .. }
        | Condition::Lore { texts, .. } => Some(texts.join(", ")),
        _ => None,
    }
}

/// Replaces the selected text condition's entries from the editor's field.
pub fn edit_texts(
    presets: &mut [Preset],
    editor: &RulesEditor,
    index: usize,
) -> Result<(), String> {
    let entries = texts(&editor.item);
    if entries.is_empty() {
        return Err("Enter the text".into());
    }
    if !editor.has_group(presets) {
        return Err("The condition is gone".into());
    }
    let Some(condition) = editor
        .selected_group_mut(presets)
        .and_then(|g| g.conditions.get_mut(index))
    else {
        return Err("The condition is gone".into());
    };
    match condition {
        Condition::DisplayName { texts, .. }
        | Condition::ItemName { texts, .. }
        | Condition::Lore { texts, .. } => {
            *texts = entries;
            Ok(())
        }
        _ => Err("Only text conditions have entries".into()),
    }
}

pub fn remove_condition(presets: &mut [Preset], editor: &RulesEditor, index: usize) -> bool {
    if !editor.has_group(presets) {
        return false;
    }
    let Some(group) = editor.selected_group_mut(presets) else {
        return false;
    };
    if index < group.conditions.len() {
        group.conditions.remove(index);
        true
    } else {
        false
    }
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

    fn preset(name: &str) -> Preset {
        Preset {
            uuid: name.to_lowercase(),
            name: name.into(),
            groups: Vec::new(),
        }
    }

    #[test]
    fn the_editor_adds_a_matcher_and_an_item_rule() {
        let mut list = profiles();
        let mut editor = RulesEditor::new(&list);
        editor.item = "swords".into();
        add(&mut list, &[], &editor).unwrap();
        editor.item = "minecraft:diamond_sword".into();
        editor.action = Action::Move;
        add(&mut list, &[], &editor).unwrap();
        let items = &list[0].items;
        assert_eq!(items[1].filter, Filter::Matcher(Matcher::Swords));
        assert_eq!(items[2].filter, Filter::Item("diamond_sword".into()));
        assert_eq!(items[2].action, Action::Move);
    }

    #[test]
    fn removing_and_cycling_keep_the_rest_in_order() {
        let mut list = profiles();
        let mut editor = RulesEditor::new(&list);
        editor.item = "swords".into();
        add(&mut list, &[], &editor).unwrap();
        assert!(cycle(&mut list, &editor, 0));
        assert_eq!(list[0].items[0].action, Action::Move);
        assert!(remove(&mut list, &editor, 1));
        assert_eq!(list[0].items.len(), 1);
        assert!(!remove(&mut list, &editor, 3));
        assert!(!cycle(&mut list, &editor, 3));
    }

    #[test]
    fn a_hotbar_slot_takes_the_editor_filter_and_round_trips() {
        let mut list = profiles();
        let mut editor = RulesEditor::new(&list);
        editor.tab = Tab::Slots;
        editor.slot = Some(3);
        editor.item = "minecraft:water_bucket".into();
        set_slot(&mut list, &editor).unwrap();
        let rule = list[0].slot_rule(3).unwrap();
        assert_eq!(rule.filter, Filter::Item("water_bucket".into()));
        editor.item.clear();
        assert!(set_slot(&mut list, &editor).is_err());
        let text = text(&list);
        let parsed = parse_profiles(&text, &[]).unwrap();
        assert_eq!(parsed[0].slot_rule(3).unwrap().filter, Filter::Item("water_bucket".into()));
    }

    #[test]
    fn presets_take_conditions_and_attach_to_item_rules() {
        let mut presets = vec![preset("Food")];
        let mut editor = RulesEditor::new(&profiles());
        editor.tab = Tab::Presets;
        editor.preset = Some("food".into());
        editor.kind = ConditionKind::Quantity;
        editor.item = "2".into();
        add_condition(&mut presets, &editor).unwrap();
        editor.kind = ConditionKind::ItemName;
        editor.item = "apple".into();
        add_condition(&mut presets, &editor).unwrap();
        let group = &presets[0].groups[0];
        assert_eq!(group.conditions.len(), 2);
        assert_eq!(summary(&group.conditions[0]), "Quantity greater or equal to 2");
        assert!(cycle_operator(&mut presets, &editor, 0));
        assert_eq!(summary(&presets[0].groups[0].conditions[0]), "Quantity less than 2");
        assert!(remove_condition(&mut presets, &editor, 1));
        assert_eq!(presets[0].groups[0].conditions.len(), 1);
        // A second AND group takes its own conditions; a text condition
        // splits its comma-separated entries.
        assert!(add_group(&mut presets, &mut editor));
        assert_eq!(editor.group, 1);
        editor.kind = ConditionKind::ItemName;
        editor.item = "apple, golden apple".into();
        add_condition(&mut presets, &editor).unwrap();
        assert_eq!(presets[0].groups[1].conditions.len(), 1);
        assert_eq!(
            summary(&presets[0].groups[1].conditions[0]),
            "Item name contains apple, golden apple"
        );
        assert_eq!(
            text_entries(&presets, &editor, 0).as_deref(),
            Some("apple, golden apple")
        );
        editor.item = "pear, melon".into();
        edit_texts(&mut presets, &editor, 0).unwrap();
        assert_eq!(
            summary(&presets[0].groups[1].conditions[0]),
            "Item name contains pear, melon"
        );
        assert!(remove_condition(&mut presets, &editor, 0));
        assert!(presets[0].groups[1].conditions.is_empty());
        // The rule's custom rule is the selected preset's uuid.
        let mut list = profiles();
        editor.tab = Tab::Items;
        editor.item = "any-food".into();
        editor.preset = Some("food".into());
        add(&mut list, &presets, &editor).unwrap();
        assert_eq!(list[0].items[1].shared.as_deref(), Some("food"));
        // The profiles reference the preset by uuid, not by inlining it.
        let profiles_json = text(&list);
        assert!(profiles_json.contains("\"food\""));
        assert!(!profiles_json.contains("apple"));
        let parsed_presets = parse_presets(&presets_text(&presets)).unwrap();
        assert_eq!(parsed_presets, presets);
        let parsed = parse_profiles(&profiles_json, &presets).unwrap();
        assert_eq!(parsed[0].items[1].preset, Some(presets[0].clone()));
        let value: Value = serde_json::from_str(&text(&list)).unwrap();
        assert_eq!(value[0]["inventoryFilters"][1]["customRule"], json!("food"));
    }

    #[test]
    fn a_new_preset_gets_a_unique_uuid_and_is_selected() {
        let mut presets = vec![preset("One"), preset("Two")];
        let mut editor = RulesEditor::new(&profiles());
        add_preset(&mut presets, &mut editor);
        assert_eq!(presets.len(), 3);
        let uuid = editor.preset.as_deref().unwrap();
        assert!(presets.iter().filter(|p| p.uuid == uuid).count() == 1);
        assert_eq!(presets_text(&presets).matches(&format!("\"{uuid}\"")).count(), 1);
        // The cycle walks the presets and ends at none.
        let mut cycle = RulesEditor::new(&profiles());
        cycle.cycle_preset(&presets);
        assert_eq!(cycle.preset.as_deref(), Some("one"));
        cycle.cycle_preset(&presets);
        assert_eq!(cycle.preset.as_deref(), Some("two"));
        cycle.cycle_preset(&presets);
        assert_eq!(cycle.preset.as_deref(), Some(uuid));
        cycle.cycle_preset(&presets);
        assert_eq!(cycle.preset, None);
    }

    #[test]
    fn the_setting_text_round_trips_the_edited_profile() {
        let mut list = profiles();
        let mut editor = RulesEditor::new(&list);
        editor.item = "any-food".into();
        add(&mut list, &[], &editor).unwrap();
        let text = text(&list);
        let parsed = parse_profiles(&text, &[]).unwrap();
        assert_eq!(parsed, list);
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0]["inventoryFilters"][1]["itemFilter"], json!("any-food"));
    }
}
