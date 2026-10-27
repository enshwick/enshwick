//! Build table ids under `ecs.`.

use super::{Constant, Symbol};

pub(crate) const ATTRIBUTE_COUNT: Constant = Constant::new("ecs.attribute_count");
pub(crate) const ATTRIBUTE_FIND: Symbol = Symbol::new("ecs.attribute_find");
pub(crate) const ATTRIBUTE_SLOT: Constant = Constant::new("ecs.attribute_slot");
pub(crate) const CHANGE_TRACKER: Constant = Constant::new("ecs.change_tracker");
pub(crate) const COMMAND_APPEND: Symbol = Symbol::new("ecs.command_append");
pub(crate) const COMMAND_BUFFERS: Constant = Constant::new("ecs.command_buffers");
pub(crate) const COMMAND_RECORDS: Constant = Constant::new("ecs.command_records");
pub(crate) const COMMAND_SET_DATA: Constant = Constant::new("ecs.command_set_data");
pub(crate) const COMMAND_SET_HASH: Constant = Constant::new("ecs.command_set_hash");
pub(crate) const COMMAND_SET_SIZE: Constant = Constant::new("ecs.command_set_size");
pub(crate) const COMPONENT_READ: Symbol = Symbol::new("ecs.component_read");
pub(crate) const COMPONENT_READ_RAW: Symbol = Symbol::new("ecs.component_read_raw");
pub(crate) const COMPONENT_READ_STATIC: Symbol = Symbol::new("ecs.component_read_static");
pub(crate) const COMPONENT_WRITE: Symbol = Symbol::new("ecs.component_write");
pub(crate) const CONTEXT_ENTITY_WORLD: Constant = Constant::new("ecs.context_entity_world");
pub(crate) const CONTEXT_ROW: Constant = Constant::new("ecs.context_row");
pub(crate) const DESCRIPTOR_ENTITY_RECORDS: Constant =
    Constant::new("ecs.descriptor_entity_records");
pub(crate) const DESTROY_REQUEST: Symbol = Symbol::new("ecs.destroy_request");
pub(crate) const ENTITIES_ENTRY_SIZE: Constant = Constant::new("ecs.entities_entry_size");
pub(crate) const ENTITIES_ENTRY_TRANSFORM: Constant = Constant::new("ecs.entities_entry_transform");
pub(crate) const ENTITIES_WITH: Symbol = Symbol::new("ecs.entities_with");
pub(crate) const ENTITY_MAP: Constant = Constant::new("ecs.entity_map");
pub(crate) const ENTITY_MAP_CAPACITY: Constant = Constant::new("ecs.entity_map_capacity");
pub(crate) const ENTITY_RECORDS: Constant = Constant::new("ecs.entity_records");
pub(crate) const ENTITY_RECORD_SIZE: Constant = Constant::new("ecs.entity_record_size");
pub(crate) const ENTITY_SLOT: Symbol = Symbol::new("ecs.entity_slot");
pub(crate) const EVENT_MERGE: Symbol = Symbol::new("ecs.event_merge");
pub(crate) const EVENT_PUSH: Symbol = Symbol::new("ecs.event_push");
pub(crate) const EVENT_RECORD_ALIGN: Constant = Constant::new("ecs.event_record_align");
pub(crate) const EVENT_RECORD_HEADER: Constant = Constant::new("ecs.event_record_header");
pub(crate) const EVENT_RECORD_SIZE: Constant = Constant::new("ecs.event_record_size");
pub(crate) const EVENT_STREAMS: Constant = Constant::new("ecs.event_streams");
pub(crate) const EVENT_STREAM_COUNT: Constant = Constant::new("ecs.event_stream_count");
pub(crate) const EVENT_STREAM_SIZE: Constant = Constant::new("ecs.event_stream_size");
pub(crate) const EVENT_STREAM_USED: Constant = Constant::new("ecs.event_stream_used");
pub(crate) const ITERATOR_INIT: Symbol = Symbol::new("ecs.iterator_init");
pub(crate) const ITERATOR_NEXT: Symbol = Symbol::new("ecs.iterator_next");
pub(crate) const LEVEL_DATA_OFFSET: Constant = Constant::new("ecs.level_data_offset");
pub(crate) const LEVEL_FLAGS: Constant = Constant::new("ecs.level_flags");
pub(crate) const MARK_CHANGED: Symbol = Symbol::new("ecs.mark_changed");
pub(crate) const RECORD_TEMPLATE: Constant = Constant::new("ecs.record_template");
pub(crate) const REGISTRY_TYPE_MAP: Constant = Constant::new("ecs.registry_type_map");
pub(crate) const REGISTRY_TYPE_MAP_FALLBACK: Constant =
    Constant::new("ecs.registry_type_map_fallback");
pub(crate) const REGISTRY_TYPE_RECORDS: Constant = Constant::new("ecs.registry_type_records");
pub(crate) const REGISTRY_TYPE_RECORD_SIZE: Constant =
    Constant::new("ecs.registry_type_record_size");
pub(crate) const ROW_ENTITY: Symbol = Symbol::new("ecs.row_entity");
pub(crate) const SLOT_ROOT: Symbol = Symbol::new("ecs.slot_root");
pub(crate) const SPAWN_REQUEST: Symbol = Symbol::new("ecs.spawn_request");
pub(crate) const TEMPLATE_GUID: Symbol = Symbol::new("ecs.template_guid");
pub(crate) const TYPE_INDEX: Symbol = Symbol::new("ecs.type_index");
pub(crate) const TYPE_INFO_HASH: Constant = Constant::new("ecs.type_info_hash");
pub(crate) const TYPE_RECORD_STATIC: Constant = Constant::new("ecs.type_record_static");
