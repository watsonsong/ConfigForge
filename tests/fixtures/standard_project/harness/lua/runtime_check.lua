local root = assert(os.getenv("LUA_CONFIG_ROOT"), "LUA_CONFIG_ROOT should be set")
package.path = root .. "/?.lua;" .. root .. "/?/init.lua;" .. package.path

function globalTranslate(context, msgid)
  local catalog = {
    ["items.name\004Iron Ore"] = "铁矿石",
    ["classes.name\004Warrior"] = "战士",
    ["classes.description\004Durable melee fighter."] = "耐久的近战战士。",
  }
  return catalog[context .. "\004" .. msgid]
end

local config = require("reference_game_config")

assert(package.loaded["reference_game_config.items"] == nil, "items should be lazy-loaded")

local items = config.items()
assert(package.loaded["reference_game_config.items"] ~= nil, "items should be loaded after access")
local item = assert(items.get_by_id(2001), "item 2001 should be indexed")
assert(item.id == 2001, "item id field access failed")
assert(item.name == "Iron Ore", "item text field access failed")
assert(item:name_text() == "铁矿石", "item text translation failed")
assert(item.price == 5, "item price field access failed")
assert(item.stack.max_stack == 999, "fragment field access failed")
assert(items.get_by_id(404) == nil, "missing item should return nil")

local class_levels = config.class_levels()
local class_level = assert(class_levels.get_by_class_level(1, 2), "composite index should resolve")
assert(class_level.attributes.max_health == 145, "nested struct field access failed")
assert(class_level.attributes.attack == 16, "nested struct second field access failed")
local class = assert(class_level:class(config), "row foreign key link should resolve")
assert(class:name_text() == "战士", "linked row text translation failed")
assert(class:description_text() == "耐久的近战战士。", "linked row description translation failed")

local surnames = config.character_surnames()
assert(#surnames.rows == 4, "source-order surname table length failed")
assert(surnames.rows[1].value == "Stone", "source-order surname first value failed")
local male_given_names = config.character_male_given_names()
assert(male_given_names.rows[2].value == "Borin", "source-order male given name failed")
local female_given_names = config.character_female_given_names()
assert(female_given_names.rows[3].value == "Celeste", "source-order female given name failed")

local game_rules = config.game_rules()
assert(game_rules.max_player_level == 60, "object table int field failed")
assert(game_rules.pvp_enabled == true, "object table bool field failed")
assert(#game_rules.starter_item_ids == 2, "object table list length failed")
assert(game_rules.starter_item_ids[1] == 2001, "object table list first value failed")
assert(game_rules.starter_item_ids[2] == 2002, "object table list second value failed")
assert(game_rules.starter_reward.gold == 500, "object table fixed reward gold failed")
assert(#game_rules.starter_reward.item_ids == 2, "object table fixed reward item list length failed")
assert(game_rules.starter_reward.item_ids[1] == 2001, "object table fixed reward item list first value failed")
assert(game_rules.starter_reward.item_ids[2] == 2002, "object table fixed reward item list second value failed")
local default_class = assert(game_rules.default_class(config), "object foreign key link should resolve")
assert(default_class.class_id == 1, "object single link target field failed")
local starter_items = game_rules.starter_items(config)
assert(#starter_items == 2, "object list link length failed")
assert(starter_items[1].id == 2001, "object list link first item failed")
assert(starter_items[2].id == 2002, "object list link second item failed")

config.preload_all()
assert(package.loaded["reference_game_config.classes"] ~= nil, "preload_all should load classes")
