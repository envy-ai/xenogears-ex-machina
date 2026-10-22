//! Thin Studio package operations over the shared runtime implementation.
use crate::StudioError;
use serde::Deserialize;
use serde_json::{Value, json};
use xem_disc::mods::{BuildFile, BuildSpec, Package, PackageSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildArgs {
    pub spec: BuildSpec,
    pub files: Vec<BuildFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectArgs {
    pub bytes: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleArgs {
    pub packages: Vec<Vec<u8>>,
}
fn error(error: xem_disc::Error) -> StudioError {
    StudioError::new("invalid_package", error.to_string())
}
pub fn build(args: BuildArgs) -> Result<Value, StudioError> {
    let bytes = Package::build(args.spec, args.files).map_err(error)?;
    let package = Package::decode(&bytes).map_err(error)?;
    Ok(json!({"bytes":bytes,"identity":package.identity(),"manifest":package.manifest()}))
}
pub fn inspect(args: InspectArgs) -> Result<Value, StudioError> {
    let package = Package::decode(&args.bytes).map_err(error)?;
    Ok(json!({"identity":package.identity(),"manifest":package.manifest()}))
}
pub fn bundle(args: BundleArgs) -> Result<Value, StudioError> {
    let packages = args
        .packages
        .iter()
        .map(|bytes| Package::decode(bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(error)?;
    let set = PackageSet::new(packages).map_err(error)?;
    Ok(json!({"bytes":set.to_bytes(),"identities":set.identities()}))
}
