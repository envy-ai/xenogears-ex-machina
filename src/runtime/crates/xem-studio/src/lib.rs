//! Host-independent studio commands, shared by native callers and wasm-bindgen.
//!
//! See tools/studio/README.md for the JSON protocol. No game data is loaded by
//! this service without an explicit field or disc command.

pub mod disc;
pub mod field;
pub mod names;
pub mod packages;
pub mod packed;

#[cfg(feature = "test-fixtures")]
mod package_fixture_overlay;

use serde::Deserialize;
use serde_json::{Value, json};
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Exercise the shared package preparation path against a deliberately synthetic disc.
///
/// This fixture-only entrypoint is excluded from default builds and refuses every
/// boot path except the invented SLUS_999.99;1 test image.
#[cfg(feature = "test-fixtures")]
pub fn package_fixture_overlay(image: &[u8], packages: &[u8], slot: u32) -> Result<String, String> {
    package_fixture_overlay::run(image, packages, slot)
}

/// WASM counterpart used only by the isolated fixture-enabled parity package.
#[cfg(all(feature = "test-fixtures", target_arch = "wasm32"))]
#[wasm_bindgen(js_name = package_fixture_overlay)]
pub fn package_fixture_overlay_wasm(
    image: &[u8],
    packages: &[u8],
    slot: u32,
) -> Result<String, JsValue> {
    package_fixture_overlay(image, packages, slot).map_err(|error| JsValue::from_str(&error))
}

const OPERATIONS: &[&str] = &[
    "version",
    "echo",
    "field.decode",
    "field.encode",
    "disc.open",
    "disc.poll",
    "disc.missing",
    "disc.close",
    "disc.maps",
    "field.map_events",
    "field.map_bundle",
    "field.structure",
    "names.validate",
    "field.lanes",
    "disc.cue",
    "package.build",
    "package.inspect",
    "package.bundle",
    "field.draft.open",
    "field.templates",
    "field.instruction",
    "field.draft.edit",
    "field.draft.undo",
    "field.draft.redo",
    "field.draft.compile",
    "field.draft.export",
    "field.draft.import",
    "field.draft.close",
    "field.draft.snapshot",
    "field.draft.repack",
];

#[derive(Deserialize)]
#[serde(
    tag = "op",
    content = "args",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Command {
    Version,
    Echo(Value),
    #[serde(rename = "field.draft.open")]
    DraftOpen(FieldEncodeArgs),
    #[serde(rename = "field.templates")]
    FieldTemplates,
    #[serde(rename = "field.instruction")]
    FieldInstruction(field::draft::InstructionArgs),
    #[serde(rename = "field.draft.edit")]
    DraftEdit(DraftEditArgs),
    #[serde(rename = "field.draft.undo")]
    DraftUndo(DraftRevisionArgs),
    #[serde(rename = "field.draft.redo")]
    DraftRedo(DraftRevisionArgs),
    #[serde(rename = "field.draft.compile")]
    DraftCompile(DraftArgs),
    #[serde(rename = "field.draft.export")]
    DraftExport(DraftArgs),
    #[serde(rename = "field.draft.import")]
    DraftImport(DraftImportArgs),
    #[serde(rename = "field.draft.close")]
    DraftClose(DraftArgs),
    #[serde(rename = "field.draft.snapshot")]
    DraftSnapshot(DraftArgs),
    #[serde(rename = "field.draft.repack")]
    DraftRepack(DraftRepackArgs),
    #[serde(rename = "field.decode")]
    FieldDecode(FieldDecodeArgs),
    #[serde(rename = "field.encode")]
    FieldEncode(FieldEncodeArgs),
    #[serde(rename = "disc.open")]
    DiscOpen(disc::OpenArgs),
    #[serde(rename = "disc.poll")]
    DiscPoll(SessionArgs),
    #[serde(rename = "disc.missing")]
    DiscMissing(SessionArgs),
    #[serde(rename = "disc.close")]
    DiscClose(SessionArgs),
    #[serde(rename = "disc.maps")]
    DiscMaps(SessionArgs),
    #[serde(rename = "field.map_events")]
    FieldMapEvents(MapArgs),
    #[serde(rename = "field.map_bundle")]
    FieldMapBundle(MapArgs),
    #[serde(rename = "field.structure")]
    FieldStructure(FieldStructureArgs),
    #[serde(rename = "names.validate")]
    NamesValidate(NamesArgs),
    #[serde(rename = "field.lanes")]
    FieldLanes(FieldLanesArgs),
    #[serde(rename = "disc.cue")]
    DiscCue(CueArgs),
    #[serde(rename = "package.build")]
    PackageBuild(packages::BuildArgs),
    #[serde(rename = "package.inspect")]
    PackageInspect(packages::InspectArgs),
    #[serde(rename = "package.bundle")]
    PackageBundle(packages::BundleArgs),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftArgs {
    draft: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftRepackArgs {
    draft: u32,
    bundle: Vec<u8>,
    context: Option<xem_disc::field_context::SourceContext>,
    source: Option<xem_disc::field_context::SourceIdentity>,
    module_sha256: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftRevisionArgs {
    draft: u32,
    revision: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftEditArgs {
    draft: u32,
    revision: u32,
    action: field::draft::Action,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftImportArgs {
    document: Box<field::draft::Document>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CueArgs {
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldStructureArgs {
    graph: Box<field::FieldGraph>,
    actor: usize,
    event: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldLanesArgs {
    graph: Box<field::FieldGraph>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NamesArgs {
    json: String,
    #[serde(default)]
    context: names::NamesContext,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionArgs {
    session: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MapArgs {
    session: u32,
    map: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldDecodeArgs {
    bytes: Vec<u8>,
    declared_size: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldEncodeArgs {
    graph: Box<field::FieldGraph>,
}

impl Command {
    fn parse(input: &str) -> Result<Self, StudioError> {
        let value: Value = serde_json::from_str(input)
            .map_err(|error| StudioError::new("malformed_json", error.to_string()))?;
        let op = value.get("op").and_then(Value::as_str).ok_or_else(|| {
            StudioError::new(
                "invalid_command",
                "expected an object with a string `op` field",
            )
        })?;
        if !OPERATIONS.contains(&op) {
            return Err(StudioError::new(
                "unknown_operation",
                format!("unknown operation `{op}`"),
            ));
        }
        serde_json::from_value(value)
            .map_err(|error| StudioError::new("invalid_args", error.to_string()))
    }

    fn execute(self, studio: &Studio) -> Result<CommandResult, StudioError> {
        Ok(match self {
            Self::DraftRepack(args) => {
                let drafts = studio.drafts.borrow();
                let draft = drafts.get(args.draft)?;
                let compiled = draft.compile()?;
                let base = &draft.document().base;
                let bytes = match (&args.context, &args.source, &args.module_sha256) {
                    (None, None, None) => disc::replace_events(&args.bundle, base, &compiled)?,
                    (Some(context), Some(source), Some(module)) => {
                        disc::replace_events_with_context(
                            &args.bundle,
                            base,
                            &compiled,
                            context,
                            source,
                            module,
                        )?
                    }
                    _ => {
                        return Err(StudioError::new(
                            "invalid_args",
                            "measured repack requires context, selected source identity and module_sha256 together",
                        ));
                    }
                };
                CommandResult::Json(json!({"bytes":bytes,"source_sha256":base.source_sha256}))
            }
            Self::FieldInstruction(args) => {
                CommandResult::Json(json!(field::draft::instruction_template(args)?))
            }
            Self::FieldTemplates => CommandResult::Json(field::draft::templates()?),
            Self::DraftOpen(args) => {
                CommandResult::Json(studio.drafts.borrow_mut().open(*args.graph)?)
            }
            Self::DraftImport(args) => {
                CommandResult::Json(studio.drafts.borrow_mut().import(*args.document)?)
            }
            Self::DraftEdit(args) => {
                let mut drafts = studio.drafts.borrow_mut();
                let draft = drafts.get_mut(args.draft)?;
                draft.edit(args.revision, args.action)?;
                CommandResult::Json(draft.snapshot(args.draft)?)
            }
            Self::DraftUndo(args) => {
                let mut drafts = studio.drafts.borrow_mut();
                let draft = drafts.get_mut(args.draft)?;
                draft.undo(args.revision)?;
                CommandResult::Json(draft.snapshot(args.draft)?)
            }
            Self::DraftRedo(args) => {
                let mut drafts = studio.drafts.borrow_mut();
                let draft = drafts.get_mut(args.draft)?;
                draft.redo(args.revision)?;
                CommandResult::Json(draft.snapshot(args.draft)?)
            }
            Self::DraftCompile(args) => {
                CommandResult::Json(json!(studio.drafts.borrow().get(args.draft)?.compile()?))
            }
            Self::DraftExport(args) => {
                CommandResult::Json(json!(studio.drafts.borrow().get(args.draft)?.document()))
            }
            Self::DraftSnapshot(args) => CommandResult::Json(
                studio
                    .drafts
                    .borrow()
                    .get(args.draft)?
                    .snapshot(args.draft)?,
            ),
            Self::DraftClose(args) => {
                studio.drafts.borrow_mut().close(args.draft)?;
                CommandResult::Json(json!({"closed":args.draft}))
            }
            Self::PackageBuild(args) => CommandResult::Json(packages::build(args)?),
            Self::PackageInspect(args) => CommandResult::Json(packages::inspect(args)?),
            Self::PackageBundle(args) => CommandResult::Json(packages::bundle(args)?),
            Self::DiscCue(args) => {
                if args.text.len() > 65536 {
                    return Err(StudioError::new("invalid_args", "cue sheet exceeds 64 KiB"));
                }
                let track = xem_disc::parse_cue(&args.text)
                    .map_err(|error| StudioError::new("invalid_cue", error.to_string()))?;
                CommandResult::Json(json!({"file": track.file, "number": track.number,
                    "start": track.start, "end": track.end}))
            }
            Self::Version => CommandResult::Version {
                version: env!("CARGO_PKG_VERSION"),
                operations: OPERATIONS,
            },
            Self::Echo(value) => CommandResult::Echo(value),
            Self::FieldDecode(args) => {
                CommandResult::FieldGraph(Box::new(field::decode(&args.bytes, args.declared_size)?))
            }
            Self::FieldEncode(args) => CommandResult::FieldBytes(field::encode(&args.graph)?),
            Self::DiscOpen(args) => CommandResult::Json(studio.discs.borrow_mut().open(args)?),
            Self::DiscPoll(args) => {
                CommandResult::Json(studio.discs.borrow_mut().poll(args.session)?)
            }
            Self::DiscMissing(args) => {
                CommandResult::Json(studio.discs.borrow_mut().missing(args.session)?)
            }
            Self::DiscClose(args) => {
                CommandResult::Json(studio.discs.borrow_mut().close(args.session)?)
            }
            Self::DiscMaps(args) => {
                CommandResult::Json(studio.discs.borrow_mut().maps(args.session)?)
            }
            Self::FieldMapBundle(args) => CommandResult::Json(
                studio
                    .discs
                    .borrow_mut()
                    .map_bundle(args.session, args.map)?,
            ),
            Self::FieldMapEvents(args) => CommandResult::Json(
                studio
                    .discs
                    .borrow_mut()
                    .map_events(args.session, args.map)?,
            ),
            Self::FieldStructure(args) => CommandResult::Json(json!(field::structure::structure(
                &args.graph,
                args.actor,
                args.event
            )?)),
            Self::NamesValidate(args) => {
                CommandResult::Json(json!(names::validate(&args.json, &args.context)?))
            }
            Self::FieldLanes(args) => CommandResult::Json(json!(field::lanes::lanes(&args.graph)?)),
        })
    }
}

enum CommandResult {
    Version {
        version: &'static str,
        operations: &'static [&'static str],
    },
    Echo(Value),
    FieldGraph(Box<field::FieldGraph>),
    FieldBytes(Vec<u8>),
    Json(Value),
}

impl CommandResult {
    fn into_json(self) -> Value {
        match self {
            Self::Version {
                version,
                operations,
            } => json!({ "version": version, "operations": operations }),
            Self::Echo(value) => value,
            Self::FieldGraph(graph) => json!(graph),
            Self::FieldBytes(bytes) => json!(bytes),
            Self::Json(value) => value,
        }
    }
}

/// An operation's explicit failure, preserved across the JSON boundary.
#[derive(Debug)]
pub struct StudioError {
    pub kind: &'static str,
    pub message: String,
}

impl StudioError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn into_json(self) -> Value {
        json!({ "error": { "kind": self.kind, "message": self.message } })
    }
}

impl From<field::FieldError> for StudioError {
    fn from(error: field::FieldError) -> Self {
        Self::new(error.kind, error.message)
    }
}

/// The studio service. Each call returns JSON, including when input is invalid.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Default)]
pub struct Studio {
    discs: RefCell<disc::DiscSessions>,
    drafts: RefCell<field::draft::Drafts>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Studio {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        Self::default()
    }

    /// Binary range fill beside the JSON command interface. The wasm binding
    /// consumes Uint8Array, never JSON number arrays.
    pub fn disc_fill(&self, session: u32, offset: f64, data: &[u8]) -> String {
        if !offset.is_finite()
            || !(0.0..=9_007_199_254_740_991.0).contains(&offset)
            || offset.fract() != 0.0
        {
            return StudioError::new(
                "invalid_args",
                "disc fill offset must be a nonnegative safe JavaScript integer",
            )
            .into_json()
            .to_string();
        }
        match self.discs.borrow_mut().fill(session, offset as u64, data) {
            Ok(value) => json!({ "ok": value }),
            Err(error) => error.into_json(),
        }
        .to_string()
    }

    pub fn command(&self, input: &str) -> String {
        match Command::parse(input).and_then(|command| command.execute(self)) {
            Ok(result) => json!({ "ok": result.into_json() }),
            Err(error) => error.into_json(),
        }
        .to_string()
    }
}
