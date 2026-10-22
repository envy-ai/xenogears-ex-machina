//! Native file adapter for the same data-only builder exposed by Studio.
use serde::Deserialize;
use std::{
    error::Error,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use xem_disc::mods::{
    BuildFile, BuildSpec, MAX_PACKAGE_BYTES, Package, PackageSet, delta::MAX_FILE_BYTES,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    spec: BuildSpec,
    files: Vec<File>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    disc: String,
    slot: u32,
    source: Option<PathBuf>,
    result: PathBuf,
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(format!("{} exceeds {limit} bytes", path.display()).into());
    }
    Ok(bytes)
}
fn write(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.first().and_then(|v|v.to_str()) {
        Some("build") if args.len()==3=>{
            let recipe_path=Path::new(&args[1]);let base=recipe_path.parent().unwrap_or(Path::new("."));
            let recipe:Recipe=serde_json::from_slice(&read(recipe_path,1<<20)?)?;
            if recipe.files.len()>1024 {return Err("too many mod files".into());}
            let mut files=Vec::new();let mut total=0usize;
            for file in recipe.files {
                let input=BuildFile{disc:file.disc,slot:file.slot,source:file.source.map(|p|read(&base.join(p),MAX_FILE_BYTES)).transpose()?,result:read(&base.join(file.result),MAX_FILE_BYTES)?};
                total+=input.result.len()+input.source.as_ref().map_or(0,Vec::len);
                if total>128<<20 {return Err("mod builder inputs exceed 128 MiB".into());}
                files.push(input);
            }
            let bytes=Package::build(recipe.spec,files)?;
            write(Path::new(&args[2]),&bytes)?;
            println!("{}",Package::decode(&bytes)?.identity());
        }
        Some("inspect") if args.len()==2=>{
            let package=Package::decode(&read(Path::new(&args[1]),MAX_PACKAGE_BYTES)?)?;
            println!("{}",serde_json::to_string_pretty(&serde_json::json!({"identity":package.identity(),"manifest":package.manifest()}))?);
        }
        Some("bundle") if args.len()>=2=>{
            let packages=args[2..].iter().map(|p|Ok(Package::decode(&read(Path::new(p),MAX_PACKAGE_BYTES)?)?)).collect::<Result<Vec<_>,Box<dyn Error>>>()?;
            let set=PackageSet::new(packages)?;write(Path::new(&args[1]),&set.to_bytes())?;
            println!("{}",serde_json::to_string(&set.identities())?);
        }
        _=>return Err("usage: xem-package build RECIPE.json NEW.xemmod | inspect FILE.xemmod | bundle NEW.xemmods [FILE.xemmod ...]".into()),
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xem-package: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
