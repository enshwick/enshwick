//! Build table ids under `ui.`.

use super::{Constant, Symbol};

pub(crate) const ACTION_BOUND: Constant = Constant::new("ui.action_bound");
pub(crate) const ACTION_EXTENT: Constant = Constant::new("ui.action_extent");
/// The game's label of a runtime `GameplayAction`, the action in, its
/// `LocaTagId` out; the key glyph calls it (`0x140d361a0`, client 26430625).
pub(crate) const ACTION_LABEL: Symbol = Symbol::new("ui.action_label");
pub(crate) const ACTION_NEW: Symbol = Symbol::new("ui.action_new");
pub(crate) const ACTION_NODE: Constant = Constant::new("ui.action_node");
pub(crate) const ACTION_QUERY: Symbol = Symbol::new("ui.action_query");
pub(crate) const ATTRIBUTE_LIST_ROWS: Constant = Constant::new("ui.attribute_list_rows");
pub(crate) const ATTRIBUTE_ROW_CELL: Constant = Constant::new("ui.attribute_row_cell");
pub(crate) const ATTRIBUTE_ROW_CELL_SECOND: Constant =
    Constant::new("ui.attribute_row_cell_second");
pub(crate) const ATTRIBUTE_ROW_HOVERED: Constant = Constant::new("ui.attribute_row_hovered");
pub(crate) const ATTRIBUTE_ROW_HOVERED_FLAG: Constant =
    Constant::new("ui.attribute_row_hovered_flag");
pub(crate) const ATTRIBUTE_ROW_HOVERED_TOOLTIP: Constant =
    Constant::new("ui.attribute_row_hovered_tooltip");
pub(crate) const ATTRIBUTE_ROW_LABEL_LENGTH: Constant =
    Constant::new("ui.attribute_row_label_length");
pub(crate) const ATTRIBUTE_ROW_MILLISECONDS: Constant =
    Constant::new("ui.attribute_row_milliseconds");
pub(crate) const ATTRIBUTE_ROW_PAIR_AFTER_SECOND: Constant =
    Constant::new("ui.attribute_row_pair_after_second");
pub(crate) const ATTRIBUTE_ROW_PERCENT: Constant = Constant::new("ui.attribute_row_percent");
pub(crate) const ATTRIBUTE_ROW_PLUS: Constant = Constant::new("ui.attribute_row_plus");
pub(crate) const ATTRIBUTE_ROW_PUSH: Symbol = Symbol::new("ui.attribute_row_push");
pub(crate) const ATTRIBUTE_ROW_SECOND: Constant = Constant::new("ui.attribute_row_second");
pub(crate) const ATTRIBUTE_ROW_SECONDS: Constant = Constant::new("ui.attribute_row_seconds");
pub(crate) const ATTRIBUTE_ROW_TREND: Constant = Constant::new("ui.attribute_row_trend");
pub(crate) const ATTRIBUTE_ROW_VALUE: Constant = Constant::new("ui.attribute_row_value");
pub(crate) const ATTRIBUTE_ROW_SIZE: Constant = Constant::new("ui.attribute_row_size");
/// The Character screen's row builders, one per group, in the order of
/// `StatGroup::ALL`.
pub(crate) const ATTRIBUTE_ROWS: [Symbol; enshwick_core::stats::StatGroup::ALL.len()] = [
    Symbol::new("ui.attribute_rows_main"),
    Symbol::new("ui.attribute_rows_fog_survival"),
    Symbol::new("ui.attribute_rows_primary"),
    Symbol::new("ui.attribute_rows_regeneration"),
    Symbol::new("ui.attribute_rows_misc"),
    Symbol::new("ui.attribute_rows_damage"),
    Symbol::new("ui.attribute_rows_weapon_damage"),
    Symbol::new("ui.attribute_rows_enemy_damage"),
    Symbol::new("ui.attribute_rows_protection"),
];
pub(crate) const BUTTON: Symbol = Symbol::new("ui.button");
pub(crate) const BUTTON_ALIGN: Constant = Constant::new("ui.button_align");
pub(crate) const BUTTON_CENTERED: Constant = Constant::new("ui.button_centered");
pub(crate) const BUTTON_ENABLED: Constant = Constant::new("ui.button_enabled");
pub(crate) const BUTTON_EXTENT: Constant = Constant::new("ui.button_extent");
pub(crate) const BUTTON_PADDING_X: Constant = Constant::new("ui.button_padding_x");
pub(crate) const BUTTON_PADDING_Y: Constant = Constant::new("ui.button_padding_y");
pub(crate) const BUTTON_SCALE: Constant = Constant::new("ui.button_scale");
pub(crate) const BUTTON_SIZE_MODE: Constant = Constant::new("ui.button_size_mode");
pub(crate) const BUTTON_SOUND: Constant = Constant::new("ui.button_sound");
pub(crate) const BUTTON_KIND: Constant = Constant::new("ui.button_kind");
pub(crate) const BUTTON_ICON: Constant = Constant::new("ui.button_icon");
pub(crate) const BUTTON_ICON_SHOWN: Constant = Constant::new("ui.button_icon_shown");
pub(crate) const COLUMN: Symbol = Symbol::new("ui.column");
pub(crate) const CONTEXT: Symbol = Symbol::new("ui.context");
pub(crate) const CONTEXT_BUNDLE: Constant = Constant::new("ui.context_bundle");
pub(crate) const CONTEXT_HOST: Constant = Constant::new("ui.context_host");
pub(crate) const CONTEXT_INPUT: Constant = Constant::new("ui.context_input");
pub(crate) const CONTEXT_NODES: Constant = Constant::new("ui.context_nodes");
pub(crate) const CONTEXT_NAV: Constant = Constant::new("ui.context_nav");
pub(crate) const CONTEXT_PLAYER: Constant = Constant::new("ui.context_player");
pub(crate) const CONTEXT_STATE: Constant = Constant::new("ui.context_state");
pub(crate) const DROPDOWN: Symbol = Symbol::new("ui.dropdown");
pub(crate) const DROPDOWN_CAPACITY: Constant = Constant::new("ui.dropdown_capacity");
pub(crate) const DROPDOWN_COUNT: Constant = Constant::new("ui.dropdown_count");
pub(crate) const DROPDOWN_ENABLED: Constant = Constant::new("ui.dropdown_enabled");
pub(crate) const DROPDOWN_ENTRY_TEXT: Constant = Constant::new("ui.dropdown_entry_text");
pub(crate) const DROPDOWN_EXTERNAL: Constant = Constant::new("ui.dropdown_external");
pub(crate) const DROPDOWN_EXTENT: Constant = Constant::new("ui.dropdown_extent");
pub(crate) const DROPDOWN_INIT: Symbol = Symbol::new("ui.dropdown_init");
pub(crate) const ENTRY_SIZE: Constant = Constant::new("ui.entry_size");
pub(crate) const ENTRY_TEXT: Constant = Constant::new("ui.entry_text");
pub(crate) const ENTRY_VALUE: Constant = Constant::new("ui.entry_value");
pub(crate) const FIRST_USE: Symbol = Symbol::new("ui.first_use");
pub(crate) const FOCUS_REQUEST: Symbol = Symbol::new("ui.focus_request");
pub(crate) const FRAME_CLOSE: Symbol = Symbol::new("ui.frame_close");
pub(crate) const FRAME_COLOR: Symbol = Symbol::new("ui.frame_color");
pub(crate) const FRAME_EXTENT: Constant = Constant::new("ui.frame_extent");
pub(crate) const FRAME_FROM_NODE: Symbol = Symbol::new("ui.frame_from_node");
pub(crate) const FRAME_HIT: Symbol = Symbol::new("ui.frame_hit");
pub(crate) const FRAME_NODE: Constant = Constant::new("ui.frame_node");
pub(crate) const FRAME_OPEN: Symbol = Symbol::new("ui.frame_open");
pub(crate) const FRAME_STRETCH: Symbol = Symbol::new("ui.frame_stretch");
pub(crate) const HOST_ANSWER: Constant = Constant::new("ui.host_answer");
pub(crate) const HOST_ANSWERED: Constant = Constant::new("ui.host_answered");
pub(crate) const HUD: Symbol = Symbol::new("ui.hud");
pub(crate) const HUD_SCREEN: Constant = Constant::new("ui.hud_screen");
pub(crate) const PROMPT_CONTEXT: Constant = Constant::new("ui.prompt_context");
pub(crate) const PROMPT_SITE: Symbol = Symbol::new("ui.prompt_site");
pub(crate) const PROMPT_TITLE: Constant = Constant::new("ui.prompt_title");
pub(crate) const PROMPT_VALID: Constant = Constant::new("ui.prompt_valid");
pub(crate) const PROMPT_VERB: Constant = Constant::new("ui.prompt_verb");
pub(crate) const SCREEN_HOST: Constant = Constant::new("ui.screen_host");
pub(crate) const HUD_VISIBLE: Symbol = Symbol::new("ui.hud_visible");
pub(crate) const INPUT_HINT: Symbol = Symbol::new("ui.input_hint");
pub(crate) const INPUT_NAVIGATING: Constant = Constant::new("ui.input_navigating");
/// The interface's posters of menu commands: each reserves its command in
/// the UI queue, writes its kind byte and returns it, or null when the
/// queue is full (client 26430625).
pub(crate) const POST_BACK: Symbol = Symbol::new("ui.post_back");
pub(crate) const POST_JOIN: Symbol = Symbol::new("ui.post_join");
pub(crate) const POST_PASSWORD: Symbol = Symbol::new("ui.post_password");
pub(crate) const POST_START_PLAY: Symbol = Symbol::new("ui.post_start_play");
/// The posters a host uses: the character (0x11), the world (0x16), the
/// session's kind (0x27) and the play of the chosen world (0x28).
pub(crate) const POST_CHARACTER: Symbol = Symbol::new("ui.post_character");
pub(crate) const POST_WORLD: Symbol = Symbol::new("ui.post_world");
pub(crate) const POST_SESSION: Symbol = Symbol::new("ui.post_session");
pub(crate) const POST_PLAY_WORLD: Symbol = Symbol::new("ui.post_play_world");
/// Where the main menu reads a host's payload (`0x140873710`, client
/// 26430625): the character's slot id, the world's slot id, each a `u16`,
/// and the session's kind byte.
pub(crate) const POST_CHARACTER_SLOT: Constant = Constant::new("ui.post_character_slot");
pub(crate) const POST_WORLD_SLOT: Constant = Constant::new("ui.post_world_slot");
pub(crate) const POST_SESSION_KIND: Constant = Constant::new("ui.post_session_kind");
/// Where the main menu reads a posted command's payload (`0x140873710`,
/// client 26430625): the password's text and its `u16` length, the join's
/// area byte (its target follows it) and the address's last `u16`.
pub(crate) const POST_PASSWORD_TEXT: Constant = Constant::new("ui.post_password_text");
pub(crate) const POST_PASSWORD_LEN: Constant = Constant::new("ui.post_password_len");
pub(crate) const POST_JOIN_AREA: Constant = Constant::new("ui.post_join_area");
pub(crate) const POST_JOIN_TAIL: Constant = Constant::new("ui.post_join_tail");
pub(crate) const ITEM_SLOT: Symbol = Symbol::new("ui.item_slot");
pub(crate) const ITEM_SLOT_CLICKABLE: Constant = Constant::new("ui.item_slot_clickable");
pub(crate) const ITEM_SLOT_COUNT: Constant = Constant::new("ui.item_slot_count");
pub(crate) const ITEM_SLOT_FOCUSABLE: Constant = Constant::new("ui.item_slot_focusable");
pub(crate) const ITEM_SLOT_HIT: Constant = Constant::new("ui.item_slot_hit");
pub(crate) const ITEM_SLOT_FOCUSED: Constant = Constant::new("ui.item_slot_focused");
pub(crate) const ITEM_SLOT_POINTER: Constant = Constant::new("ui.item_slot_pointer");
pub(crate) const ITEM_SLOT_INIT: Symbol = Symbol::new("ui.item_slot_init");
pub(crate) const ITEM_SLOT_INIT_END: Constant = Constant::new("ui.item_slot_init_end");
pub(crate) const ITEM_SLOT_PRESSED: Constant = Constant::new("ui.item_slot_pressed");
pub(crate) const ITEM_SLOT_RECORD_SIZE: Constant = Constant::new("ui.item_slot_record_size");
pub(crate) const ITEM_SLOT_STATE: Constant = Constant::new("ui.item_slot_state");
pub(crate) const ITEM_SLOT_STATE_LOCAL: Constant = Constant::new("ui.item_slot_state_local");
pub(crate) const LABEL_ALIGN: Constant = Constant::new("ui.label_align");
pub(crate) const LABEL_DRAW: Symbol = Symbol::new("ui.label_draw");
pub(crate) const LABEL_EXTENT: Constant = Constant::new("ui.label_extent");
pub(crate) const LABEL_FLAGS: Constant = Constant::new("ui.label_flags");
pub(crate) const LABEL_FONT_SIZE: Constant = Constant::new("ui.label_font_size");
pub(crate) const LABEL_NEW: Symbol = Symbol::new("ui.label_new");
pub(crate) const LABEL_SHOWN: Constant = Constant::new("ui.label_shown");
pub(crate) const LEGAL_LINE_SITE: Symbol = Symbol::new("ui.legal_line_site");
pub(crate) const LOCA_ID: Symbol = Symbol::new("ui.loca_id");
pub(crate) const LOCA_TEXT: Symbol = Symbol::new("ui.loca_text");
pub(crate) const MAIN_ENTRY: Symbol = Symbol::new("ui.main_entry");
pub(crate) const MAIN_MENU_SITE: Symbol = Symbol::new("ui.main_menu_site");
pub(crate) const MENU_ENTRY_EXTENT: Constant = Constant::new("ui.menu_entry_extent");
pub(crate) const MENU_ENTRY_FONT: Constant = Constant::new("ui.menu_entry_font");
pub(crate) const MENU_ENTRY_TEXT: Constant = Constant::new("ui.menu_entry_text");
pub(crate) const NAV_AXIS: Constant = Constant::new("ui.nav_axis");
pub(crate) const NAV_CONFIG: Constant = Constant::new("ui.nav_config");
pub(crate) const NAV_CONFIG_SIZE: Constant = Constant::new("ui.nav_config_size");
pub(crate) const NAV_DROP: Symbol = Symbol::new("ui.nav_drop");
pub(crate) const NAV_END: Symbol = Symbol::new("ui.nav_end");
pub(crate) const NAV_EXTENT: Constant = Constant::new("ui.nav_extent");
pub(crate) const NAV_FLAGS: Constant = Constant::new("ui.nav_flags");
pub(crate) const NAV_HIGHLIGHT: Constant = Constant::new("ui.nav_highlight");
pub(crate) const NAV_INIT: Symbol = Symbol::new("ui.nav_init");
pub(crate) const NAV_INPUT: Symbol = Symbol::new("ui.nav_input");
pub(crate) const NAV_INPUT_KIND: Constant = Constant::new("ui.nav_input_kind");
pub(crate) const NAV_KIND: Constant = Constant::new("ui.nav_kind");
pub(crate) const NAV_LAYER: Constant = Constant::new("ui.nav_layer");
pub(crate) const NAV_PUSH: Symbol = Symbol::new("ui.nav_push");
pub(crate) const NAV_SET: Symbol = Symbol::new("ui.nav_set");
pub(crate) const NAV_SOUND: Constant = Constant::new("ui.nav_sound");
pub(crate) const NAV_WRAP: Constant = Constant::new("ui.nav_wrap");
pub(crate) const NODE_ALIGN: Symbol = Symbol::new("ui.node_align");
pub(crate) const NODE_FIND: Symbol = Symbol::new("ui.node_find");
pub(crate) const NODE_HEIGHT: Symbol = Symbol::new("ui.node_height");
pub(crate) const NODE_ID: Constant = Constant::new("ui.node_id");
pub(crate) const NODE_ID_GET: Symbol = Symbol::new("ui.node_id_get");
pub(crate) const NODE_LAID_HEIGHT: Constant = Constant::new("ui.node_laid_height");
pub(crate) const NODE_MIN_HEIGHT: Symbol = Symbol::new("ui.node_min_height");
pub(crate) const NODE_PADDING: Symbol = Symbol::new("ui.node_padding");
pub(crate) const NODE_SPACING: Symbol = Symbol::new("ui.node_spacing");
pub(crate) const NODE_STRETCH: Symbol = Symbol::new("ui.node_stretch");
pub(crate) const NODE_WIDTH: Symbol = Symbol::new("ui.node_width");
pub(crate) const NO_CONSTRUCT: Symbol = Symbol::new("ui.no_construct");
pub(crate) const PANEL: Symbol = Symbol::new("ui.panel");
pub(crate) const PANEL_ALPHA: Constant = Constant::new("ui.panel_alpha");
pub(crate) const PANEL_EXTENT: Constant = Constant::new("ui.panel_extent");
pub(crate) const PANEL_KIND: Constant = Constant::new("ui.panel_kind");
pub(crate) const PASS_BINDINGS: Symbol = Symbol::new("ui.pass_bindings");
pub(crate) const PASS_COUNT: Constant = Constant::new("ui.pass_count");
pub(crate) const PASS_HOLDER: Constant = Constant::new("ui.pass_holder");
pub(crate) const PASS_KEY_SIZE: Constant = Constant::new("ui.pass_key_size");
pub(crate) const PASS_KEYS: Constant = Constant::new("ui.pass_keys");
pub(crate) const PASS_ON: Constant = Constant::new("ui.pass_on");
pub(crate) const PASS_REFRESH: Symbol = Symbol::new("ui.pass_refresh");
pub(crate) const PASS_SET: Symbol = Symbol::new("ui.pass_set");
pub(crate) const PAUSE_ENTRY: Symbol = Symbol::new("ui.pause_entry");
pub(crate) const PAUSE_MENU_SITE: Symbol = Symbol::new("ui.pause_menu_site");
pub(crate) const PERSIST: Symbol = Symbol::new("ui.persist");
pub(crate) const PLAYER_DATA: Constant = Constant::new("ui.player_data");
pub(crate) const PLAYER_ITEMS: Constant = Constant::new("ui.player_items");
pub(crate) const POPUP: Symbol = Symbol::new("ui.popup");
pub(crate) const POPUP_ACTIVE: Constant = Constant::new("ui.popup_active");
pub(crate) const POPUP_BUTTONS: Constant = Constant::new("ui.popup_buttons");
pub(crate) const POPUP_DETAIL: Constant = Constant::new("ui.popup_detail");
pub(crate) const POPUP_DETAIL_LEN: Constant = Constant::new("ui.popup_detail_len");
pub(crate) const POPUP_EXTENT: Constant = Constant::new("ui.popup_extent");
pub(crate) const POPUP_MESSAGE: Constant = Constant::new("ui.popup_message");
pub(crate) const POPUP_TITLE: Constant = Constant::new("ui.popup_title");
pub(crate) const POPUP_WINDOW: Symbol = Symbol::new("ui.popup_window");
pub(crate) const POPUP_WINDOW_LEN: Constant = Constant::new("ui.popup_window_len");
pub(crate) const PROGRESS: Symbol = Symbol::new("ui.progress");
pub(crate) const SCROLLBAR_LOOK: Symbol = Symbol::new("ui.scrollbar_look");
pub(crate) const SCROLL_BEGIN: Symbol = Symbol::new("ui.scroll_begin");
pub(crate) const SCROLL_CONFIG: Symbol = Symbol::new("ui.scroll_config");
pub(crate) const SCROLL_CONFIG_EXTENT: Constant = Constant::new("ui.scroll_config_extent");
pub(crate) const SCROLL_END: Symbol = Symbol::new("ui.scroll_end");
pub(crate) const SCROLL_EXTENT: Constant = Constant::new("ui.scroll_extent");
pub(crate) const SCROLL_FOCUS: Symbol = Symbol::new("ui.scroll_focus");
pub(crate) const SCROLL_INPUT: Symbol = Symbol::new("ui.scroll_input");
pub(crate) const SCROLL_NAV: Symbol = Symbol::new("ui.scroll_nav");
pub(crate) const SCROLL_SET_FOCUS: Symbol = Symbol::new("ui.scroll_set_focus");
pub(crate) const SCROLL_STATE_SIZE: Constant = Constant::new("ui.scroll_state_size");
pub(crate) const SLIDER: Symbol = Symbol::new("ui.slider");
pub(crate) const SLIDER_BOUNDED: Constant = Constant::new("ui.slider_bounded");
pub(crate) const SLIDER_DISCRETE: Constant = Constant::new("ui.slider_discrete");
pub(crate) const SLIDER_EXTENT: Constant = Constant::new("ui.slider_extent");
pub(crate) const SLIDER_FORMAT: Constant = Constant::new("ui.slider_format");
pub(crate) const SLIDER_INIT: Symbol = Symbol::new("ui.slider_init");
pub(crate) const SLIDER_MAX: Constant = Constant::new("ui.slider_max");
pub(crate) const SLIDER_MIN: Constant = Constant::new("ui.slider_min");
pub(crate) const SLIDER_ON_RELEASE: Constant = Constant::new("ui.slider_on_release");
pub(crate) const SLIDER_READ_ONLY: Constant = Constant::new("ui.slider_read_only");
pub(crate) const SLIDER_STEPS: Constant = Constant::new("ui.slider_steps");
pub(crate) const SLIDER_STOPS: Constant = Constant::new("ui.slider_stops");
pub(crate) const SLIDER_STOP_CAPACITY: Constant = Constant::new("ui.slider_stop_capacity");
pub(crate) const SLIDER_STOP_COUNT: Constant = Constant::new("ui.slider_stop_count");
pub(crate) const SOUND: Symbol = Symbol::new("ui.sound");
pub(crate) const STATE_SCREEN: Constant = Constant::new("ui.state_screen");
pub(crate) const TEXT_FIELD: Symbol = Symbol::new("ui.text_field");
pub(crate) const TEXT_FIELD_EDITING: Constant = Constant::new("ui.text_field_editing");
pub(crate) const TEXT_FIELD_EXTENT: Constant = Constant::new("ui.text_field_extent");
pub(crate) const TEXT_FIELD_INPUT: Symbol = Symbol::new("ui.text_field_input");
pub(crate) const ROW: Symbol = Symbol::new("ui.row");
pub(crate) const SEPARATOR: Symbol = Symbol::new("ui.separator");
pub(crate) const SEPARATOR_HEIGHT: Constant = Constant::new("ui.separator_height");
pub(crate) const SEPARATOR_MARGIN: Constant = Constant::new("ui.separator_margin");
pub(crate) const SEPARATOR_OFFSET: Constant = Constant::new("ui.separator_offset");
pub(crate) const TEXT_FIELD_LOOK: Constant = Constant::new("ui.text_field_look");
pub(crate) const TEXT_FIELD_STATE: Symbol = Symbol::new("ui.text_field_state");
pub(crate) const TEXT_FIELD_STATE_KEY: Constant = Constant::new("ui.text_field_state_key");
pub(crate) const TEXT_FIELD_STATE_SIZE: Constant = Constant::new("ui.text_field_state_size");
pub(crate) const TEXT_FIELD_START: Constant = Constant::new("ui.text_field_start");
/// Takes a text field's caller text into its kept state, `(field)`
/// (client 26430625, `0x140d60310`).
pub(crate) const TEXT_FIELD_TAKE: Symbol = Symbol::new("ui.text_field_take");
pub(crate) const TEXT_FIELD_TEXT: Constant = Constant::new("ui.text_field_text");
pub(crate) const TOGGLE: Symbol = Symbol::new("ui.toggle");
pub(crate) const TOGGLE_COUNT: Constant = Constant::new("ui.toggle_count");
pub(crate) const TOGGLE_ENABLED: Constant = Constant::new("ui.toggle_enabled");
pub(crate) const TOGGLE_EXPLAIN: Constant = Constant::new("ui.toggle_explain");
pub(crate) const TOGGLE_EXPLAINER: Constant = Constant::new("ui.toggle_explainer");
pub(crate) const TOGGLE_EXTENT: Constant = Constant::new("ui.toggle_extent");
pub(crate) const TOGGLE_LABEL: Constant = Constant::new("ui.toggle_label");
pub(crate) const TOOLTIP_PANEL_SITE: Symbol = Symbol::new("ui.tooltip_panel_site");
pub(crate) const TOOLTIP_PLAIN_SITE: Symbol = Symbol::new("ui.tooltip_plain_site");
pub(crate) const VIEW_SLOT: Constant = Constant::new("ui.view_slot");
pub(crate) const VIEWER_ITEM: Constant = Constant::new("ui.viewer_item");
pub(crate) const VIEWER_LAYOUT: Constant = Constant::new("ui.viewer_layout");
pub(crate) const VIEWER_PIDE: Constant = Constant::new("ui.viewer_pide");
pub(crate) const VIEWER_UNKNOWN: Constant = Constant::new("ui.viewer_unknown");
pub(crate) const WINDOW_BEGIN: Symbol = Symbol::new("ui.window_begin");
pub(crate) const WINDOW_END: Symbol = Symbol::new("ui.window_end");
pub(crate) const WINDOW_EXTENT: Constant = Constant::new("ui.window_extent");
pub(crate) const WINDOW_NODE: Constant = Constant::new("ui.window_node");
pub(crate) const WINDOWS: Symbol = Symbol::new("ui.windows");
pub(crate) const WINDOWS_VIEW: Constant = Constant::new("ui.windows_view");
pub(crate) const MENU_TAB: Constant = Constant::new("ui.menu_tab");
pub(crate) const RECIPE_DIRECTORY: Symbol = Symbol::new("ui.recipe_directory");
pub(crate) const RECIPE_HIGHLIGHTED: Constant = Constant::new("ui.recipe_highlighted");
pub(crate) const RECIPE_SELECT: Constant = Constant::new("ui.recipe_select");
pub(crate) const RETURN_SCREEN: Constant = Constant::new("ui.return_screen");
pub(crate) const SCREEN_SHOWS_CURSOR: Symbol = Symbol::new("ui.screen_shows_cursor");
pub(crate) const TOGGLE_HUD_SCREEN: Symbol = Symbol::new("ui.toggle_hud_screen");
