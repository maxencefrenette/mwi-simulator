use std::collections::HashMap;

use serde::Deserialize;

use crate::domain::{MarketSnapshot, Observation, OpenOrder, OrderSide, State};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PlayerExport {
    #[serde(default)]
    pub derived: DerivedPlayerState,
    #[serde(default, rename = "characterItemMap")]
    pub character_item_map: HashMap<String, CharacterItem>,
    #[serde(default, rename = "characterSkillMap")]
    pub character_skill_map: HashMap<String, CharacterSkill>,
    #[serde(default, rename = "actionDetailMaps")]
    pub action_detail_maps: HashMap<String, HashMap<String, ActionDetail>>,
    #[serde(default, rename = "skillingActionTypeBuffsDict")]
    pub skilling_action_type_buffs_dict: HashMap<String, Option<Vec<Buff>>>,
    #[serde(default, rename = "skillingActionHridBuffsDict")]
    pub skilling_action_hrid_buffs_dict: HashMap<String, Option<Vec<Buff>>>,
    #[serde(default, rename = "actionTypeDrinkSlotsDict")]
    pub action_type_drink_slots_dict: HashMap<String, Vec<Option<DrinkSlot>>>,
    #[serde(default, rename = "itemDetailDict")]
    pub item_detail_dict: HashMap<String, ItemDetail>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DerivedPlayerState {
    #[serde(default)]
    pub cash: f64,
    #[serde(default)]
    pub inventory: Vec<DerivedInventoryItem>,
    #[serde(default, rename = "openOrders")]
    pub open_orders: Vec<DerivedOpenOrder>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DerivedInventoryItem {
    pub item: String,
    pub quantity: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DerivedOpenOrder {
    pub side: OrderSide,
    pub item: String,
    #[serde(default)]
    pub quantity: f64,
    #[serde(default, rename = "limitPrice")]
    pub limit_price: f64,
    #[serde(default, rename = "lockedCash")]
    pub locked_cash: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterItem {
    #[serde(rename = "itemHrid")]
    pub item_hrid: String,
    #[serde(default, rename = "enhancementLevel")]
    pub enhancement_level: u32,
    #[serde(default)]
    pub count: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterSkill {
    #[serde(rename = "skillHrid")]
    pub skill_hrid: String,
    #[serde(default)]
    pub level: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionDetail {
    pub hrid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "baseTimeCost")]
    pub base_time_cost: f64,
    #[serde(default, rename = "levelRequirement")]
    pub level_requirement: Option<LevelRequirement>,
    #[serde(default, rename = "inputItems")]
    pub input_items: Option<Vec<FixedItem>>,
    #[serde(default, rename = "outputItems")]
    pub output_items: Option<Vec<FixedItem>>,
    #[serde(default, rename = "dropTable")]
    pub drop_table: Option<Vec<DropItem>>,
    #[serde(default, rename = "essenceDropTable")]
    pub essence_drop_table: Option<Vec<DropItem>>,
    #[serde(default, rename = "rareDropTable")]
    pub rare_drop_table: Option<Vec<DropItem>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelRequirement {
    #[serde(rename = "skillHrid")]
    pub skill_hrid: String,
    pub level: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixedItem {
    #[serde(rename = "itemHrid")]
    pub item_hrid: String,
    pub count: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DropItem {
    #[serde(rename = "itemHrid")]
    pub item_hrid: String,
    #[serde(rename = "dropRate")]
    pub drop_rate: f64,
    #[serde(rename = "minCount")]
    pub min_count: f64,
    #[serde(rename = "maxCount")]
    pub max_count: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Buff {
    #[serde(rename = "typeHrid")]
    pub type_hrid: String,
    #[serde(default, rename = "flatBoost")]
    pub flat_boost: f64,
    #[serde(default, rename = "ratioBoost")]
    pub ratio_boost: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DrinkSlot {
    #[serde(rename = "itemHrid")]
    pub item_hrid: String,
    #[serde(default, rename = "isActive")]
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemDetail {
    #[serde(default, rename = "sellPrice")]
    pub sell_price: f64,
}

pub fn state_from_export(player: &PlayerExport, market: &MarketSnapshot) -> State {
    let inventory = player
        .derived
        .inventory
        .iter()
        .filter(|item| !is_coin(&item.item))
        .map(|item| (item.item.clone(), item.quantity))
        .collect::<HashMap<_, _>>();
    let open_orders = player
        .derived
        .open_orders
        .iter()
        .map(|order| OpenOrder {
            side: order.side,
            item: order.item.clone(),
            remaining_quantity: order.quantity,
            limit_price: order.limit_price,
            locked_cash: order.locked_cash,
        })
        .collect();
    let visible_wealth = crate::domain::pessimistic_wealth(
        &State {
            day: 0,
            cash: player.derived.cash,
            inventory,
            open_orders,
            fixed_wealth: 0.0,
        },
        market,
    );
    let total_wealth = crate::wealth::calculate_wealth(player, market).total;
    let mut state = State {
        day: 0,
        cash: player.derived.cash,
        inventory: player
            .derived
            .inventory
            .iter()
            .filter(|item| !is_coin(&item.item))
            .map(|item| (item.item.clone(), item.quantity))
            .collect(),
        open_orders: player
            .derived
            .open_orders
            .iter()
            .map(|order| OpenOrder {
                side: order.side,
                item: order.item.clone(),
                remaining_quantity: order.quantity,
                limit_price: order.limit_price,
                locked_cash: order.locked_cash,
            })
            .collect(),
        fixed_wealth: (total_wealth - visible_wealth).max(0.0),
    };
    state.inventory.retain(|_, quantity| *quantity > 0.0);
    state
}

pub fn export_for_observation(template: &PlayerExport, observation: &Observation) -> PlayerExport {
    let mut player = template.clone();
    player.derived.cash = observation.state.cash;
    player.derived.inventory = observation
        .state
        .inventory
        .iter()
        .map(|(item, quantity)| DerivedInventoryItem {
            item: item.clone(),
            quantity: *quantity,
        })
        .collect();
    player.derived.open_orders = observation
        .state
        .open_orders
        .iter()
        .map(|order| DerivedOpenOrder {
            side: order.side,
            item: order.item.clone(),
            quantity: order.remaining_quantity,
            limit_price: order.limit_price,
            locked_cash: order.locked_cash,
        })
        .collect();
    player
}

fn is_coin(item: &str) -> bool {
    item == "coin" || item.ends_with("/coin")
}
