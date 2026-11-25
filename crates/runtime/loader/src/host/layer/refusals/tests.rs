use enshwick_core::abi::Status;

use super::{Policy, Refusal};
use crate::actions::chat::ChatOffError;
use crate::actions::commands::RunCommandError;
use crate::actions::console::{AccountActionError, ConsoleError};
use crate::actions::local_held::HeldError;
use crate::base::calls::RunCallError;
use crate::base::carrier::CarrierError;
use crate::base::emits::EmitError;
use crate::base::net::NetError;
use crate::base::notices::NoticeError;
use crate::base::points::RunPointError;
use crate::base::saves::{HoldSavesError, SaveError};
use crate::base::services::SendEventError;
use crate::base::stop::StopError;
use crate::content::content_renames::RenameError;
use crate::content::data_changes::DataChangeError;
use crate::content::item_info::ItemInfoError;
use crate::content::rates::RateError;
use crate::content::recipes::RecipeRuleError;
use crate::content::served::BuildResourceError;
use crate::content::textures::TextureError;
use crate::engine::chains::LinkError;
use crate::engine::fuse::FuseError;
use crate::engine::registry::JoinFeatureError;
use crate::engine::ticks::SetEveryError;
use crate::engine::world::WorldError;
use crate::inventory::drops::DropError;
use crate::inventory::item_data::UseItemDataError;
use crate::inventory::stats::{RollAffixesError, StatsError};
use crate::inventory::transactions::SubmitInventoryError;
use crate::knowledge::progress::WriteProgressError;
use crate::ui::interface::{DialogError, MarkersError};
use crate::ui::widget_checks::WidgetError;
use crate::ui::windows::ShowWindowError;
use crate::world::altars::NoSizerError;
use crate::world::bases::BasesError;
use crate::world::buffs::BuffError;
use crate::world::daytime::DaytimeError;
use crate::world::entities::EntityError;
use crate::world::fog::{FogReadError, UncoverError};
use crate::world::fog_image::ReadError;
use crate::world::game_settings::GameSettingsError;
use crate::world::label::LabelError;
#[cfg(windows)]
use crate::world::permissions::PermissionError;
use crate::world::prop_holds::HoldError;
#[cfg(windows)]
use crate::world::respawn::RespawnError;
use crate::world::rights::RightsError;
use crate::world::scene::SceneError;
use crate::world::spawns::SpawnError;
use crate::world::terrain::TerrainError;
use crate::world::transactions::TransactionError;
use crate::world::voxels::VoxelError;
use crate::world::weather::WeatherError;
use crate::world::writes::WriteError;

/// The constant's name of `status`, so the snapshot reads as the ABI does.
fn name(status: Status) -> String {
    [
        (Status::OK, "OK"),
        (Status::MISSING, "MISSING"),
        (Status::AMBIGUOUS, "AMBIGUOUS"),
        (Status::REFUSED, "REFUSED"),
        (Status::BAD_ARGS, "BAD_ARGS"),
        (Status::TOO_LATE, "TOO_LATE"),
        (Status::FAULTED, "FAULTED"),
        (Status::WRONG_THREAD, "WRONG_THREAD"),
        (Status::NOT_LOADED, "NOT_LOADED"),
        (Status::OUTSIDE_WORLD, "OUTSIDE_WORLD"),
        (Status::STATIC_HALF, "STATIC_HALF"),
        (Status::NOT_A_PROP, "NOT_A_PROP"),
        (Status::NOT_KEPT, "NOT_KEPT"),
        (Status::UNAVAILABLE, "UNAVAILABLE"),
        (Status::BUILDING, "BUILDING"),
        (Status::LOADER_FAULT, "LOADER_FAULT"),
        (Status::BUSY, "BUSY"),
        (Status::TOO_SMALL, "TOO_SMALL"),
    ]
    .into_iter()
    .find(|&(known, _)| known == status)
    .map_or_else(|| format!("{status:?}"), |(_, name)| name.to_owned())
}

/// One line per error: its expression, the status a mod gets and the
/// loader's policy.
macro_rules! answers {
    ($($error:expr),* $(,)?) => {{
        let lines: Vec<String> = vec![$({
            let error = $error;
            let policy: Policy = error.policy();
            format!("{} => {} {policy:?}", stringify!($error), name(error.status()))
        }),*];
        lines.join("\n")
    }};
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one row per error variant: the table is the test"
)]
fn every_domain_error_answers_mods_the_status_it_answered_before() {
    let world = WorldError::InStep;
    insta::assert_snapshot!(answers![
        WorldError::Unread,
        WorldError::NoWorld,
        WorldError::NoServer,
        WorldError::UnreadForCall,
        WorldError::WrongThread,
        WorldError::InStep,
        WorldError::Absent,
        WriteError::Unavailable,
        WriteError::NoEntity,
        WriteError::Refused,
        WriteError::StaticHalf,
        WriteError::Absent,
        WriteError::World(world),
        SubmitInventoryError::Unavailable,
        SubmitInventoryError::BadOps,
        ItemInfoError::Unread,
        ItemInfoError::NoRegistry,
        ItemInfoError::NoWorld,
        ItemInfoError::NoItem,
        ShowWindowError::NotServing,
        ShowWindowError::BadChange,
        ShowWindowError::BadWindow,
        ShowWindowError::NoActions,
        ShowWindowError::Full,
        WeatherError::Unread,
        WeatherError::NoSystem,
        WeatherError::BadWeather,
        WeatherError::Fuse(FuseError::Off),
        WeatherError::World(world),
        DialogError::NotClient,
        DialogError::WrongThread,
        DialogError::Malformed,
        VoxelError::BadBox,
        VoxelError::BadVoxel,
        VoxelError::Unread,
        VoxelError::NoEditor,
        VoxelError::OutsideWorld,
        VoxelError::NotLoaded,
        LabelError::Unread,
        LabelError::Dead,
        LabelError::NoStrings,
        LabelError::Refused,
        LabelError::World(world),
        GameSettingsError::Unread,
        GameSettingsError::NoSettings,
        GameSettingsError::NoWorldKey,
        GameSettingsError::BadSize,
        GameSettingsError::BadLength,
        GameSettingsError::Fuse(FuseError::Unmade),
        GameSettingsError::World(world),
        WriteProgressError::BadKind,
        WriteProgressError::Unavailable,
        WriteProgressError::Register(JoinFeatureError::TooLate),
        WriteProgressError::NotAsked,
        WriteProgressError::NoPlayer,
        WriteProgressError::Full,
        WriteProgressError::World(world),
        SceneError::Unread,
        SceneError::NoScene,
        SceneError::Off,
        SceneError::Unreadable,
        SceneError::Reading,
        SceneError::NotClient,
        SceneError::NoTypes,
        SceneError::World(world),
        RightsError::BadBox,
        RightsError::Unread,
        RightsError::NoZones,
        DropError::Unread,
        DropError::BadRequest,
        DropError::Item(ItemInfoError::NoItem),
        DropError::Item(ItemInfoError::Unread),
        DropError::Item(ItemInfoError::NoRegistry),
        DropError::Item(ItemInfoError::NoWorld),
        DropError::World(world),
        DropError::Spawn(SpawnError::Refused),
        StatsError::TooLate,
        StatsError::NoMod,
        StatsError::BadId,
        StatsError::NotOwn,
        StatsError::Defined,
        StatsError::Full,
        StatsError::BadDef,
        StatsError::NotRunning,
        StatsError::BadStat,
        StatsError::Undefined,
        StatsError::NoAffix,
        StatsError::BadModifier,
        StatsError::NothingToWatch,
        StatsError::NotServer,
        StatsError::NotPlayer,
        StatsError::World(world),
        UncoverError::NotServer,
        UncoverError::BadCircle,
        UncoverError::NotReady,
        UncoverError::NotSent,
        FogReadError::Unopened,
        FogReadError::NoCopy,
        RollAffixesError::Unopened,
        RollAffixesError::NoItem,
        RollAffixesError::NoRoll,
        RollAffixesError::Encode,
        RollAffixesError::Kept(UseItemDataError::BadId),
        RollAffixesError::World(world),
        RollAffixesError::Item(ItemInfoError::Unread),
        EntityError::Unread,
        EntityError::NoWorld,
        EntityError::Unavailable,
        EntityError::NoEntity,
        EntityError::Player,
        EntityError::NotSpawned,
        EntityError::Refused,
        EntityError::World(world),
        TransactionError::BadBox,
        TransactionError::BadEdit,
        TransactionError::BadCount,
        TransactionError::BadPlace,
        TransactionError::BadItem,
        TransactionError::BadShape,
        TransactionError::BlueprintChanged,
        TransactionError::NotAProp,
        TransactionError::Unavailable,
        TransactionError::Gone,
        TransactionError::OutsideWorld,
        TransactionError::NotLoaded,
        TransactionError::Full,
        TransactionError::NotUndoable,
        TransactionError::NoRights,
        TransactionError::HoldsState,
        TransactionError::Changed,
        TransactionError::Voxels(VoxelError::NotLoaded),
        TransactionError::Rights(RightsError::NoZones),
        SpawnError::Unread,
        SpawnError::NoWorld,
        SpawnError::BadRequest,
        SpawnError::Refused,
        SpawnError::World(world),
        TerrainError::Unread,
        TerrainError::NoEditor,
        TerrainError::BadEdit,
        TerrainError::BadMaterial,
        TerrainError::OutsideWorld,
        TerrainError::World(world),
        TerrainError::Voxels(VoxelError::BadBox),
        DaytimeError::Unread,
        DaytimeError::NoClock,
        DaytimeError::BadFactor,
        DaytimeError::NotAsked,
        DaytimeError::SpeedOff,
        DaytimeError::World(world),
        ConsoleError::Empty,
        ConsoleError::WrongThread,
        ConsoleError::Unknown,
        MarkersError::Off,
        MarkersError::TooMany,
        MarkersError::BadPosition,
        BuffError::Unread,
        BuffError::NoRegistry,
        BuffError::BadRequest,
        BuffError::Cleansed,
        BuffError::World(world),
        BuffError::Write(WriteError::StaticHalf),
        AccountActionError::Unread,
        AccountActionError::NoAccount,
        AccountActionError::Refused,
        AccountActionError::World(world),
        RunCommandError::BadLine,
        RunCommandError::WrongThread,
        RunCommandError::Unknown,
        TextureError::BadData,
        TextureError::NotInArchive,
        BuildResourceError::NotServed,
        BuildResourceError::Building,
        BuildResourceError::NoBase,
        BuildResourceError::NotBuilt,
        BuildResourceError::SizeChanged,
        BuildResourceError::Unsealed,
        BuildResourceError::NotInArchive,
        BasesError::TooLate,
        BasesError::BadLimits,
        BasesError::BelowGame,
        BasesError::Unavailable,
        RateError::BadRate,
        RateError::NotJoined,
        RateError::Unavailable,
        RateError::TooLate,
        RateError::BadFactor,
        RateError::Undeclared,
        RecipeRuleError::BadKind,
        RecipeRuleError::BadName,
        RecipeRuleError::Unavailable,
        DataChangeError::TooLate,
        DataChangeError::TooMany,
        DataChangeError::Unknown,
        DataChangeError::BadValue,
        DataChangeError::NeedsRestart,
        EmitError::Unread,
        EmitError::NoWorld,
        EmitError::NotAllowed,
        EmitError::BadSize,
        EmitError::Refused,
        EmitError::TooMany,
        EmitError::Unavailable,
        EmitError::NoEntity,
        EmitError::NotMovable,
        EmitError::BadPosition,
        EmitError::World(world),
        NoticeError::NoClient,
        NoticeError::World(world),
        NoticeError::Net(NetError::Throttled),
        NetError::NoPeer,
        NetError::BadChannel,
        NetError::TooLong,
        NetError::NotServing,
        NetError::Throttled,
        NetError::NotCarried,
        NetError::Carrier(CarrierError::Full),
        RenameError::ToItself,
        RenameError::Cycle,
        RenameError::Twice,
        RenameError::Sealed,
        StopError::NotServer,
        StopError::Unread,
        StopError::NotRunning,
        StopError::NoThread,
        RunPointError::Unknown,
        RunPointError::NotOwner,
        RunCallError::Unsealed,
        RunCallError::Unknown,
        RunCallError::NotCaller,
        RunCallError::NullArgs,
        RunCallError::NoServer,
        RunCallError::ServerOff,
        SendEventError::Unknown,
        SendEventError::NotOwner,
        HoldSavesError::Unread,
        HoldSavesError::Off,
        CarrierError::NoLink,
        CarrierError::Full,
        CarrierError::BadPayload,
        SaveError::Unread,
        SaveError::Blocked,
        SaveError::NotAllowed,
        SaveError::NoWorld,
        SaveError::Unreadable,
        SaveError::World(world),
        SetEveryError::NoHandler,
        SetEveryError::OtherMod,
        ReadError::Outside,
        ReadError::Short,
        WidgetError::OutOfOrder,
        WidgetError::Unknown,
        WidgetError::NoEngine,
        WidgetError::WrongThread,
        WidgetError::Malformed,
        WidgetError::HudRefused,
        WidgetError::Unavailable,
        WidgetError::NoItem,
        WidgetError::TooltipRefused,
        WidgetError::TooltipFull,
        FuseError::Off,
        FuseError::Unmade,
        LinkError::Armed,
        LinkError::Malformed,
        LinkError::SlotTaken,
        LinkError::Clash,
        HeldError,
        NoSizerError,
        ChatOffError,
        HoldError,
    ]);
}

/// The variants only a Windows build has.
#[cfg(windows)]
#[test]
fn every_windows_only_domain_error_answers_mods_the_status_it_answered_before() {
    let world = WorldError::InStep;
    insta::assert_snapshot!(answers![
        VoxelError::World(world),
        RightsError::World(world),
        TransactionError::World(world),
        TransactionError::Terrain(TerrainError::OutsideWorld),
        TransactionError::Item(ItemInfoError::NoItem),
        TransactionError::Spawn(SpawnError::BadRequest),
        TransactionError::Entity(EntityError::Player),
        PermissionError::BadBits,
        PermissionError::NotInGame,
        PermissionError::World(world),
        RespawnError::Unread,
        RespawnError::NotPlayer,
        RespawnError::NoPoint,
        RespawnError::NotASpawnPoint,
        RespawnError::World(world),
        RespawnError::Emit(EmitError::BadSize),
        RespawnError::Emit(EmitError::Refused),
    ]);
}
