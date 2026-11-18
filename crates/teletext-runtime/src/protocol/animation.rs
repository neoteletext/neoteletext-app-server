//! Animation in the program protocol (neoteletext-common
//! `docs/animation.md` section 6, ANIM-21; REQ-6533, DEC-6524): atomic
//! frames, operations on objects' animation state, teletext sprites,
//! NAPLPS objects, sprite sheets, and timelines the server plays.

use alloc::vec::Vec;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

use super::read::typed;
use super::rules::{CycleOp, FrameOp, LookOp, MoveOp, OrderInLayer, OrderOp, TargetOp};
use super::sound::{not_listed, tagged, tagged_choice};
use super::{Base64, Connections, I32In, PageNumber, SubCode, U8In, U16In, U64In};

/// The most operations of an `animate` message or a timeline's step
/// (`neoteletext_rules::limits::MAX_ANIMATE_OPERATIONS`).
pub const MOST_OPERATIONS: usize = 64;
/// The most sprites of a sub-page (ANIM-7, `MAX_SPRITES`).
pub const MOST_SPRITES: usize = 32;
/// The most NAPLPS objects of a sub-page (ANIM-11, `MAX_OBJECTS`).
pub const MOST_NAPLPS_OBJECTS: usize = 64;
/// The most frames of a sprite or an object (ANIM-14,
/// `MAX_OBJECT_FRAMES`).
pub const MOST_OBJECT_FRAMES: usize = 16;
/// The most steps of a timeline (ANIM-21, `MAX_TIMELINE_STEPS`).
pub const MAX_TIMELINE_STEPS: usize = 1_024;
/// The most timelines a program has playing for a page (ANIM-21).
pub const MOST_TIMELINES: usize = 16;

/// A layer an object is drawn in, 1 to 3: layer 0 is the page itself
/// (ANIM-12).
pub type ObjectLayer = U8In<1, 3>;
/// A step's tick, or a timeline's length: 0 to 216,000.
pub type TimelineTick = U64In<0, 216_000>;
/// A NAPLPS macro's code, `0x20` to `0x7F` (ANSI X3.110-1983 section 6.2).
pub type MacroCode = U8In<0x20, 0x7F>;

/// Which mark a `frame` message is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameMarkName {
    /// The frame begins: what follows for the page is held.
    Begin,
    /// The frame ends: what was held is applied and shown on one tick.
    End,
}

/// `frame`: begins or ends an atomic frame of a sub-page (ANIM-2,
/// ANIM-3). Between a begin and its end the receiver holds everything the
/// program sends that changes what the page shows, and at the end applies
/// it all and presents the result on one tick. An end with no begin, and a
/// begin inside a frame, are refused `frame_order`; a frame whose records
/// pass 262,144 bytes is refused `frame_too_large` and what it held is
/// sent unframed.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameMessage {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// Begin or end.
    pub mark: FrameMarkName,
}

/// One operation on a control's or an object's animation state (ANIM-6),
/// by `action`.
#[derive(Clone, PartialEq, Eq, Debug, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnimationOp {
    /// Makes a control or an object shown.
    Show(TargetOp),
    /// Makes it hidden.
    Hide(TargetOp),
    /// Forces a look on a control or an object.
    Look(LookOp),
    /// Moves an object.
    Move(MoveOp),
    /// Sets an object's layer and order.
    Order(OrderOp),
    /// Shows one frame of an object.
    Frame(FrameOp),
    /// Cycles an object's frames.
    Cycle(CycleOp),
    /// Stops an object's cycle at the frame shown.
    StopCycle(TargetOp),
    /// Forgets a control's or an object's whole animation state.
    Reset(TargetOp),
}

tagged_choice!(AnimationOp, "action", {
    "show" => Show,
    "hide" => Hide,
    "look" => Look,
    "move" => Move,
    "order" => Order,
    "frame" => Frame,
    "cycle" => Cycle,
    "stop_cycle" => StopCycle,
    "reset" => Reset,
});

/// `animate`: 1 to 64 operations on the animation state of one page's
/// controls and objects, applied together (ANIM-6), for the connections
/// named that have the page selected; the server keeps nothing of them for
/// a later viewer (ANIM-21). The same message without `connections`,
/// `page` and `sub` is a command of a rule table's group (page-rules.md
/// RULE-35), where its fields may be amounts; sent directly they are
/// constants.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Animate {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names no page (DEC-6599).
    pub sub: SubCode,
    /// The operations, applied in order.
    #[schemars(length(min = 1, max = MOST_OPERATIONS))]
    pub ops: Vec<AnimationOp>,
}

/// Where a sprite's frame, a teletext object, is defined (EN 300 706
/// clause 13).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectSource {
    /// The page's own packets X/26.
    Local,
    /// The page's Public Object Page.
    Public,
    /// Its Global Public Object Page.
    Global,
}

/// The type of a teletext object (EN 300 706 clause 13.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    /// An active object.
    Active,
    /// An adaptive object.
    Adaptive,
    /// A passive object.
    Passive,
}

/// One frame of a sprite: a teletext object, named as an Object Invocation
/// triplet names it (EN 300 706 Table 28).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpriteFrame {
    /// Where the object is defined.
    pub source: ObjectSource,
    /// Its type.
    pub kind: ObjectType,
    /// Its number, 0 to 511.
    pub number: U16In<0, 511>,
}

/// A teletext sprite (ANIM-7): a numbered placement of teletext objects
/// on a sub-page, which the receiver draws over the page and can move,
/// show, hide, order and cycle.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sprite {
    /// Its number, distinct within the message: the object number the
    /// rule table and `animate` name it by.
    pub number: U16In<1, 65_535>,
    /// Whether it is shown.
    pub shown: bool,
    /// Whether pointer triggers see it.
    pub detectable: bool,
    /// Its layer, 1 to 3.
    pub layer: ObjectLayer,
    /// Its order within the layer, lowest drawn first.
    pub order: OrderInLayer,
    /// The column of its origin, −39 to 39.
    pub column: I32In<-39, 39>,
    /// The row of its origin, −24 to 24.
    pub row: I32In<-24, 24>,
    /// Its width in cells, 1 to 40.
    pub width: U8In<1, 40>,
    /// Its height in cells, 1 to 25.
    pub height: U8In<1, 25>,
    /// Its frames, 1 to 16.
    #[schemars(length(min = 1, max = MOST_OBJECT_FRAMES))]
    pub frames: Vec<SpriteFrame>,
}

/// `sprites`: all of a sub-page's teletext sprites, replacing any it had
/// (ANIM-7); an empty list removes them.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sprites {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// The sprites.
    #[schemars(length(max = MOST_SPRITES))]
    pub sprites: Vec<Sprite>,
}

/// What a NAPLPS object is (ANIM-11).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NaplpsObjectKind {
    /// Drawn by executing its frame's macro.
    Macro,
    /// A rectangle of the stream's own picture, covered while hidden.
    Region,
}

/// A NAPLPS object (ANIM-11): a numbered object in the layer NeoTeletext
/// keeps over a NAPLPS picture.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NaplpsObject {
    /// Its number, distinct within the message.
    pub number: U16In<1, 65_535>,
    /// A macro object or a region.
    pub kind: NaplpsObjectKind,
    /// Whether it is shown.
    pub shown: bool,
    /// Whether pointer triggers see it.
    pub detectable: bool,
    /// Its layer, 1 to 3.
    pub layer: ObjectLayer,
    /// Its order within the layer, lowest drawn first.
    pub order: OrderInLayer,
    /// The left of its rectangle, in the picture's 640 by 480 pixels.
    pub x: I32In<-32_768, 32_767>,
    /// The top of its rectangle.
    pub y: I32In<-32_768, 32_767>,
    /// Its width, 1 to 640.
    pub width: U16In<1, 640>,
    /// Its height, 1 to 480.
    pub height: U16In<1, 480>,
    /// Where the rectangle's top left lies in the macro's own drawing; 0
    /// for a region.
    pub origin_x: I32In<-32_768, 32_767>,
    /// See `origin_x`.
    pub origin_y: I32In<-32_768, 32_767>,
    /// A macro object's frames, 1 to 16 macro codes; empty for a region.
    #[schemars(length(max = MOST_OBJECT_FRAMES))]
    pub frames: Vec<MacroCode>,
    /// A region's cover: the colour-map entry that fills it while it is
    /// hidden; null for a macro object.
    #[serde(default)]
    pub cover: Option<U8In<0, 255>>,
    /// The macro drawn as its hover look, or null.
    #[serde(default)]
    pub hover: Option<MacroCode>,
    /// The macro drawn as its pressed look, or null.
    #[serde(default)]
    pub pressed: Option<MacroCode>,
    /// The macro drawn as its alternative look, or null.
    #[serde(default)]
    pub alternative: Option<MacroCode>,
}

/// `naplps_objects`: all of a sub-page's NAPLPS objects, replacing any it
/// had (ANIM-11); an empty list removes them. They belong to the NAPLPS
/// stream the connection has for the sub-page.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NaplpsObjects {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names none.
    pub sub: SubCode,
    /// The objects.
    #[schemars(length(max = MOST_NAPLPS_OBJECTS))]
    pub objects: Vec<NaplpsObject>,
}

/// Which renderer a sprite sheet is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetKindName {
    /// ReGIS: hidden drawings, ending with a commit.
    Regis,
    /// NAPLPS: macros, DRCS characters and textures.
    Naplps,
}

/// `sprite_sheet`: the next bytes of one page's sprite sheet (ANIM-17,
/// DEC-6599: the page number and sub-code), one stream of at most 65,536
/// bytes for a page and kind, which a receiver feeds to the page's
/// renderer before the page's own stream. It reaches the connections named
/// that have the page selected; a program sends it at each selection, as
/// the server keeps none. A sheet draws nothing itself.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpriteSheet {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: a sheet belongs to one page and sub-code (DEC-6599),
    /// never 3F7F.
    pub sub: SubCode,
    /// Which renderer it is for.
    pub kind: SheetKindName,
    /// The bytes, exact base64.
    pub data: Base64,
    /// Begin the sheet again: a new generation.
    #[serde(default)]
    pub restart: bool,
}

/// One step of a timeline: operations at a tick.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineStep {
    /// The tick, sixty a second, from the timeline's start.
    pub tick: TimelineTick,
    /// The operations, applied in order.
    #[schemars(length(min = 1, max = MOST_OPERATIONS))]
    pub ops: Vec<AnimationOp>,
}

/// `timeline`: steps of operations that the server plays on its own clock
/// (ANIM-21), sending each step when its tick is due, counted from the
/// message's arrival, and again from the start every `loop_ticks` when
/// that is given. A timeline with a playing one's `id` replaces it.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineMessage {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page: never 3F7F, which names no page (DEC-6599).
    pub sub: SubCode,
    /// Its number, by which `timeline_stop` names it.
    pub id: U16In<1, 65_535>,
    /// The ticks after which it begins again, more than every step's
    /// tick; null to play once.
    #[serde(default)]
    pub loop_ticks: Option<U64In<1, 216_000>>,
    /// The steps, in ascending order of `tick`.
    #[schemars(length(min = 1, max = MAX_TIMELINE_STEPS))]
    pub steps: Vec<TimelineStep>,
}

/// `timeline_stop`: stops a page's timeline; one that is not playing is
/// passed over.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineStop {
    /// Who it is for.
    pub connections: Connections,
    /// The page.
    pub page: PageNumber,
    /// The sub-page, never 3F7F.
    pub sub: SubCode,
    /// The timeline's number.
    pub id: U16In<1, 65_535>,
}
