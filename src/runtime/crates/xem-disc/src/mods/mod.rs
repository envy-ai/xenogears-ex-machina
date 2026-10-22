//! Versioned data-only mod packages, validated before publishing a virtual disc.
pub mod delta;
mod integrity;
mod overlay;
mod package;
pub use overlay::{Preparation, PreparedOverlay};
pub use package::{
    BuildFile, BuildSpec, DiscTarget, FileEntry, MAX_PACKAGE_BYTES, MAX_SET_BYTES, Manifest, Package, PackageSet,
    PayloadKind,
};
