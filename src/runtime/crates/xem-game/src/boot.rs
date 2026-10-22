//! Start the game natively from the user's disc, as the BIOS does: identify
//! the disc, copy the executable's text to its address, insert the disc into
//! the drive, attach the memory cards and the Kanji font (a BIOS image's, or
//! the substitute glyphs, `xem_core::bios`). Every native host (headless,
//! desktop, Android) starts the game here; [`boot`] verifies the disc as
//! Redump's dump (`xem_disc::verify`) and gives the drive's changer the
//! game's other disc when it is beside the first.

use std::error::Error;
use std::path::{Path, PathBuf};

use xem_core::control::Session;
use crate::storage::FileStorage;
use xem_core::inspect::Schema;
use xem_core::{Runtime, bios, exe};
use xem_disc::{Identification, SectorSource};

use crate::NativeModule;

#[derive(Debug, Clone, Default)]
pub struct BootOptions {
    /// Where card1.mcd and card2.mcd live; no card is inserted without it.
    pub cards: Option<PathBuf>,
    /// A BIOS image named for this run (`--bios FILE`) for the Kanji font:
    /// one that cannot be read or that `xem_core::bios::check` refuses fails
    /// the boot, naming it.
    pub bios: Option<PathBuf>,
    /// Without `bios`, the BIOS image the player imported
    /// (`storage::stored_bios`): one that cannot be read or that the check
    /// refuses stays as it is, the substitute glyphs are placed as without
    /// one, and `Services::kanji_font_notice` says why (the storage line), as
    /// a card file that does not load leaves its slot empty and says why.
    pub stored_bios: Option<PathBuf>,
    /// build/game/stubs.txt, to name functions the port does not define.
    pub stubs: Option<PathBuf>,
    /// Keep the SPU's samples for the host to play (or record).
    pub audio_output: bool,
    /// Prevalidated ordered packages; empty means byte-identical vanilla.
    pub packages: xem_disc::mods::PackageSet,
}

pub fn boot(disc: &Path, options: &BootOptions) -> Result<(Session<NativeModule>, Identification), Box<dyn Error>> {
    let mut source = xem_disc::open_path(disc)?;
    let known = xem_disc::identify(source.as_mut())?.disc.ok_or("not a known Xenogears disc")?;
    xem_disc::verify(source.as_mut(), known)?;
    let (mut session, identity) = boot_source(source, options)?;
    for (other, path, source) in xem_disc::discs_beside(disc, known) {
        match source {
            Ok(source) => {
                eprintln!("disc {} beside it: {}", other.number(), path.display());
                let (_, source) = prepare_source(source, &options.packages)?;
                session.runtime.services().add_disc(other.number(), source);
            }
            Err(error) => eprintln!("disc {} beside it not used: {}: {error}", other.number(), path.display()),
        }
    }
    Ok((session, identity))
}

/// [`boot`] from an opened disc image its host has verified
/// (`xem_disc::verify`; `xem_disc::open_reader` over a file descriptor, as
/// Android's document picker gives one).
pub fn boot_source(
    mut source: Box<dyn SectorSource>,
    options: &BootOptions,
) -> Result<(Session<NativeModule>, Identification), Box<dyn Error>> {
    let identity = xem_disc::identify(source.as_mut())?;
    identity.disc.ok_or("not a known Xenogears disc")?;
    let (executable, source) = prepare_identified(source, &identity, &options.packages)?;
    let header = exe::parse(&executable)?;

    let mut runtime = Runtime::new(NativeModule::new());
    runtime.identity.disc = xem_disc::state_identity(&identity);
    runtime.identity.packages = options.packages.identities();
    runtime.packages = options.packages.clone();
    if let Some(text) = options.stubs.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
        runtime.services().stub_names =
            text.lines().filter_map(|l| l.split_once(' ').map(|(_, n)| n.to_string())).collect();
    }
    if let Some(dir) = &options.cards {
        let binding = xem_core::save::SaveBinding::from(&runtime.identity);
        let storage = xem_core::save::BoundCardStorage::prepare(Box::new(FileStorage { dir: dir.clone() }), binding)?;
        runtime.services().card.attach_storage(Box::new(storage))?;
        for error in &runtime.services().card.errors { eprintln!("{error}"); }
    }
    let install = |runtime: &mut Runtime<NativeModule>, path: &Path| {
        let rom = xem_storage::read(path, bios::MAX_FILE).map_err(|e| e.to_string())?;
        runtime.install_kanji_font(Some(&rom))
    };
    match (&options.bios, &options.stored_bios) {
        (Some(path), _) => {
            install(&mut runtime, path).map_err(|e| format!("BIOS image {}: {e}", path.display()))?;
        }
        (None, Some(path)) => {
            if let Err(error) = install(&mut runtime, path) {
                let notice = format!("Imported BIOS image {} not used: {error}; save titles use the substitute glyphs.", path.display());
                eprintln!("{notice}");
                runtime.install_kanji_font(None)?;
                runtime.services().kanji_font_notice = Some(notice);
            }
        }
        (None, None) => {
            runtime.install_kanji_font(None)?;
            eprintln!("kanji font: substitute glyphs for save titles (no BIOS image)");
        }
    }
    runtime.services().spu.set_output(options.audio_output);
    // The BIOS copies the executable's text to its address and jumps to pc0.
    runtime.load_executable(header.text_address, exe::text(&executable, &header))?;
    runtime.services().insert_disc(identity.disc.map_or(0, |disc| disc.number()), source);
    // The module's schema (build/game/schema.json, embedded by build.rs).
    let schema = Schema::parse(include_str!(concat!(env!("OUT_DIR"), "/schema.json")))?;
    if schema.module_sha256.as_deref().is_some_and(|id| id != runtime.identity.module) {
        return Err("schema module_sha256 does not match the embedded game module".into());
    }
    let started = std::time::Instant::now();
    Ok((Session::new(runtime,Some(schema),move || started.elapsed()),identity))
}

/// Prepare only against an identified original source; never identify the
/// patched executable as if it were an original image.
fn prepare_identified(mut source: Box<dyn SectorSource>, identity: &Identification, packages: &xem_disc::mods::PackageSet) -> Result<(Vec<u8>, Box<dyn SectorSource>), Box<dyn Error>> {
    let mut preparation = packages.preparation(identity)?;
    while !preparation.advance(source.as_mut())? {}
    let overlay = preparation.overlay().ok_or("package preparation did not finish")?;
    Ok((overlay.executable().to_vec(), overlay.wrap(source)?))
}

pub fn prepare_source(mut source: Box<dyn SectorSource>, packages: &xem_disc::mods::PackageSet) -> Result<(Vec<u8>, Box<dyn SectorSource>), Box<dyn Error>> {
    let identity = xem_disc::identify(source.as_mut())?;
    identity.disc.ok_or("not a known Xenogears disc")?;
    prepare_identified(source, &identity, packages)
}

/// Load the same set container accepted by the browser. Disabled content is
/// still structurally validated; it never changes a source or state identity.
pub fn load_packages(path: Option<&Path>, disabled: bool) -> Result<xem_disc::mods::PackageSet, String> {
    let Some(path) = path else {
        if disabled { return Err("--packages-disabled requires --packages FILE".into()); }
        return Ok(Default::default());
    };
    let bytes = xem_storage::read(path, xem_disc::mods::MAX_SET_BYTES).map_err(|e| format!("{}: {e}", path.display()))?;
    let set = xem_disc::mods::PackageSet::from_bytes(&bytes).map_err(|e| e.to_string())?;
    Ok(if disabled { Default::default() } else { set })
}
